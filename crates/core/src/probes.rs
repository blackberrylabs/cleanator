//! Probes: items listed by a tool rather than found by walking the disk (R4 — a managed
//! store is read through its own tool). Every probe is read-only. A missing tool, a
//! daemon that is down or an unparsable output yields an empty result with a status the
//! report shows as "not scanned", never an error that stops the run.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

use crate::exec::{installed, run, LONG, SHORT};
use crate::paths::Env;
use crate::scan::{size_of, DirNode};
use crate::size::{parse_docker, parse_size};

#[derive(Debug, Clone, Serialize)]
pub struct ProbeItem {
    /// Stable identifier inside the probe (image id, model name, volume name…).
    pub key: String,
    pub name: String,
    pub path: Option<PathBuf>,
    /// Size to display.
    pub size: u64,
    /// Space actually freed by the cleanup, when it differs from `size`.
    pub reclaimable: Option<u64>,
    /// Unix seconds of the last use, when the tool reports it.
    pub last_used: Option<i64>,
    pub in_use: bool,
    pub note: Option<String>,
    /// Substitution fields for the rule's command template (`{name}`, `{tag}`…).
    pub fields: BTreeMap<String, String>,
}

impl ProbeItem {
    fn new(key: impl Into<String>, name: impl Into<String>, size: u64) -> ProbeItem {
        ProbeItem {
            key: key.into(),
            name: name.into(),
            path: None,
            size,
            reclaimable: None,
            last_used: None,
            in_use: false,
            note: None,
            fields: BTreeMap::new(),
        }
    }

    fn field(mut self, k: &str, v: impl Into<String>) -> ProbeItem {
        self.fields.insert(k.to_string(), v.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", content = "detail", rename_all = "kebab-case")]
pub enum ProbeStatus {
    Ok,
    ToolMissing(String),
    Failed(String),
    NotImplemented(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub probe: String,
    pub item: Option<String>,
    pub status: ProbeStatus,
    pub items: Vec<ProbeItem>,
}

impl ProbeResult {
    fn empty(probe: &str, item: Option<&str>, status: ProbeStatus) -> ProbeResult {
        ProbeResult {
            probe: probe.to_string(),
            item: item.map(str::to_string),
            status,
            items: Vec::new(),
        }
    }
}

/// What the probes need: the environment, the scanned tree (to size paths without a
/// second walk) and a cache of tool outputs so Docker is queried once, not per rule.
pub struct ProbeContext<'a> {
    pub env: &'a Env,
    pub tree: Option<&'a DirNode>,
    docker: Mutex<Option<Option<DockerData>>>,
    ollama_loaded: Mutex<Option<HashSet<String>>>,
}

impl<'a> ProbeContext<'a> {
    pub fn new(env: &'a Env, tree: Option<&'a DirNode>) -> ProbeContext<'a> {
        ProbeContext {
            env,
            tree,
            docker: Mutex::new(None),
            ollama_loaded: Mutex::new(None),
        }
    }

    /// Size and dates of a path, from the scanned tree when it is inside it, otherwise by
    /// a dedicated read-only walk.
    pub fn size_of_path(&self, path: &Path) -> Option<(u64, i64, i64)> {
        if let Some(node) = self.tree.and_then(|t| t.find(path)) {
            return Some((node.size, node.newest_mtime, node.newest_atime));
        }
        size_of(path).map(|n| (n.size, n.newest_mtime, n.newest_atime))
    }

    fn docker(&self) -> Option<DockerData> {
        let mut guard = self.docker.lock().unwrap();
        if guard.is_none() {
            *guard = Some(DockerData::load());
        }
        guard.as_ref().unwrap().clone()
    }

    /// Host paths bind-mounted into running containers (for the in-use checks).
    pub fn docker_bind_mounts(&self) -> Vec<PathBuf> {
        self.docker()
            .map(|d| d.bind_mounts.clone())
            .unwrap_or_default()
    }

    fn ollama_loaded(&self) -> HashSet<String> {
        let mut guard = self.ollama_loaded.lock().unwrap();
        if guard.is_none() {
            let mut set = HashSet::new();
            if let Some(out) = run("ollama", &["ps"], SHORT) {
                if out.ok() {
                    for line in out.stdout.lines().skip(1) {
                        if let Some(name) = line.split_whitespace().next() {
                            set.insert(name.to_string());
                        }
                    }
                }
            }
            *guard = Some(set);
        }
        guard.as_ref().unwrap().clone()
    }
}

/// Probes that only read the filesystem (no external tool): they run even when tool
/// probes are disabled.
pub fn probe_is_filesystem_only(name: &str) -> bool {
    matches!(name, "hf-cache" | "nvm-ls")
}

/// Run the probe named in a rule.
pub fn run_probe(
    ctx: &ProbeContext,
    probe: &str,
    item: Option<&str>,
    paths: &[String],
    roots: &[String],
    item_glob: Option<&str>,
    exclude_under: &[String],
) -> ProbeResult {
    match probe {
        "docker" => docker_probe(ctx, item),
        "docker-containers" => docker_running(ctx),
        "ollama" => ollama_probe(ctx, paths),
        "ollama-ps" => {
            let mut r = ProbeResult::empty("ollama-ps", None, ProbeStatus::Ok);
            r.items = ctx
                .ollama_loaded()
                .into_iter()
                .map(|n| ProbeItem::new(n.clone(), n, 0))
                .collect();
            r
        }
        "hf-cache" => hf_cache_probe(ctx, paths, item_glob.unwrap_or("models--*")),
        "snap-list-all" => snap_probe(),
        "journalctl-disk-usage" => journal_probe(),
        "nvm-ls" => nvm_probe(ctx, paths),
        "git-status" => git_probe(ctx, roots, exclude_under),
        "conda-env-list" => conda_envs_probe(ctx),
        "conda-pkgs-dirs" => conda_pkgs_probe(ctx),
        "pnpm-store-path" => pnpm_probe(ctx),
        "apt-autoremove-dry-run" => apt_autoremove_probe(),
        "flatpak-unused" => ProbeResult::empty(
            probe,
            item,
            ProbeStatus::NotImplemented(
                "flatpak has no read-only listing of unused runtimes; run `flatpak uninstall --unused` yourself to see them".into(),
            ),
        ),
        other => ProbeResult::empty(other, item, ProbeStatus::NotImplemented("unknown probe".into())),
    }
}

// ------------------------------------------------------------------ Docker

#[derive(Debug, Clone, Default)]
struct DockerImage {
    id: String,
    repository: String,
    tag: String,
    unique_size: u64,
    size: u64,
    containers: u32,
    created_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct DockerContainer {
    id: String,
    name: String,
    image: String,
    status: String,
    size: u64,
}

#[derive(Debug, Clone, Default)]
struct DockerVolume {
    name: String,
    links: u32,
    size: u64,
}

#[derive(Debug, Clone, Default)]
struct DockerData {
    images: Vec<DockerImage>,
    containers: Vec<DockerContainer>,
    volumes: Vec<DockerVolume>,
    build_cache_size: u64,
    build_cache_reclaimable: u64,
    bind_mounts: Vec<PathBuf>,
}

fn s(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .map(|x| match x {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}

impl DockerData {
    fn load() -> Option<DockerData> {
        if !installed("docker") {
            return None;
        }
        let verbose = run("docker", &["system", "df", "-v", "--format", "json"], LONG)?;
        if !verbose.ok() {
            return None;
        }
        let v: serde_json::Value = serde_json::from_str(verbose.stdout.trim()).ok()?;
        let mut data = DockerData::default();
        for img in v
            .get("Images")
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
        {
            data.images.push(DockerImage {
                id: s(img, "ID"),
                repository: s(img, "Repository"),
                tag: s(img, "Tag"),
                unique_size: parse_docker(&s(img, "UniqueSize")).unwrap_or(0),
                size: parse_docker(&s(img, "Size")).unwrap_or(0),
                containers: s(img, "Containers").parse().unwrap_or(0),
                created_at: parse_docker_date(&s(img, "CreatedAt")),
            });
        }
        for c in v
            .get("Containers")
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
        {
            data.containers.push(DockerContainer {
                id: s(c, "ContainerID"),
                name: s(c, "Names"),
                image: s(c, "Image"),
                status: s(c, "Status"),
                size: parse_docker(s(c, "Size").split_whitespace().next().unwrap_or("0"))
                    .unwrap_or(0),
            });
        }
        for vol in v
            .get("Volumes")
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
        {
            data.volumes.push(DockerVolume {
                name: s(vol, "Name"),
                links: s(vol, "Links").parse().unwrap_or(0),
                size: parse_docker(&s(vol, "Size")).unwrap_or(0),
            });
        }
        if let Some(summary) = run("docker", &["system", "df", "--format", "json"], LONG) {
            for line in summary.stdout.lines() {
                let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                if s(&row, "Type") == "Build Cache" {
                    data.build_cache_size = parse_docker(&s(&row, "Size")).unwrap_or(0);
                    let recl = s(&row, "Reclaimable");
                    data.build_cache_reclaimable =
                        parse_docker(recl.split_whitespace().next().unwrap_or("0")).unwrap_or(0);
                }
            }
        }
        if let Some(ps) = run("docker", &["ps", "-q"], SHORT) {
            let ids: Vec<&str> = ps.stdout.split_whitespace().collect();
            if !ids.is_empty() {
                let mut args = vec!["inspect", "--format", "{{json .Mounts}}"];
                args.extend(ids.iter().copied());
                if let Some(ins) = run("docker", &args, LONG) {
                    for line in ins.stdout.lines() {
                        let Ok(mounts) = serde_json::from_str::<serde_json::Value>(line) else {
                            continue;
                        };
                        for m in mounts.as_array().into_iter().flatten() {
                            if s(m, "Type") == "bind" {
                                let src = s(m, "Source");
                                if !src.is_empty() {
                                    data.bind_mounts.push(PathBuf::from(src));
                                }
                            }
                        }
                    }
                }
            }
        }
        Some(data)
    }

    fn containers_of(&self, img: &DockerImage) -> Vec<&DockerContainer> {
        let reference = format!("{}:{}", img.repository, img.tag);
        self.containers
            .iter()
            .filter(|c| {
                c.image == reference
                    || (!img.id.is_empty()
                        && (c.image.starts_with(&img.id) || img.id.starts_with(&c.image)))
            })
            .collect()
    }
}

fn parse_docker_date(text: &str) -> Option<i64> {
    // "2026-10-13 10:22:41 +0200 CEST"
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let s = format!("{} {} {}", parts[0], parts[1], parts[2]);
    chrono::DateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S %z")
        .ok()
        .map(|d| d.timestamp())
}

fn docker_probe(ctx: &ProbeContext, item: Option<&str>) -> ProbeResult {
    let Some(data) = ctx.docker() else {
        let status = if installed("docker") {
            ProbeStatus::Failed(
                "docker daemon not reachable (is it running, are you in the docker group?)".into(),
            )
        } else {
            ProbeStatus::ToolMissing("docker".into())
        };
        return ProbeResult::empty("docker", item, status);
    };
    let mut result = ProbeResult::empty("docker", item, ProbeStatus::Ok);
    match item.unwrap_or("") {
        "build-cache" => {
            if data.build_cache_size > 0 {
                let mut it =
                    ProbeItem::new("build-cache", "Docker build cache", data.build_cache_size);
                it.reclaimable = Some(data.build_cache_reclaimable);
                result.items.push(it);
            }
        }
        "dangling-images" => {
            for img in data
                .images
                .iter()
                .filter(|i| i.repository == "<none>" && i.containers == 0)
            {
                let mut it = ProbeItem::new(
                    img.id.clone(),
                    format!("<none> ({})", &img.id[..img.id.len().min(12)]),
                    img.unique_size,
                )
                .field("id", img.id.clone());
                it.last_used = img.created_at;
                result.items.push(it);
            }
        }
        "unused-tagged-images" => {
            for img in data.images.iter().filter(|i| i.repository != "<none>") {
                let users = data.containers_of(img);
                let all_created =
                    !users.is_empty() && users.iter().all(|c| c.status.starts_with("Created"));
                if img.containers != 0 && !all_created {
                    continue;
                }
                let reference = format!("{}:{}", img.repository, img.tag);
                let mut it = ProbeItem::new(reference.clone(), reference, img.unique_size)
                    .field("repository", img.repository.clone())
                    .field("tag", img.tag.clone())
                    .field("id", img.id.clone());
                it.last_used = img.created_at;
                if all_created {
                    let names: Vec<&str> = users.iter().map(|c| c.name.as_str()).collect();
                    it.note = Some(format!(
                        "used only by container {} (created, never started): remove that container first",
                        names.join(", ")
                    ));
                }
                if img.size != img.unique_size {
                    it.note = Some(format!(
                        "{}{} of its {} are layers shared with other images and stay on disk",
                        it.note
                            .as_deref()
                            .map(|n| format!("{n}; "))
                            .unwrap_or_default(),
                        crate::size::human(img.size - img.unique_size),
                        crate::size::human(img.size)
                    ));
                }
                result.items.push(it);
            }
        }
        "stopped-containers" => {
            for c in data.containers.iter().filter(|c| {
                c.status.starts_with("Exited")
                    || c.status.starts_with("Created")
                    || c.status.starts_with("Dead")
            }) {
                let mut it = ProbeItem::new(c.id.clone(), c.name.clone(), c.size)
                    .field("name", c.name.clone())
                    .field("id", c.id.clone());
                it.note = Some(format!("{} — image {}", c.status, c.image));
                result.items.push(it);
            }
        }
        "unused-volumes" => {
            for v in data.volumes.iter().filter(|v| v.links == 0) {
                let anonymous = v.name.len() == 64 && v.name.chars().all(|c| c.is_ascii_hexdigit());
                let mut it = ProbeItem::new(v.name.clone(), v.name.clone(), v.size)
                    .field("name", v.name.clone());
                it.note = Some(if anonymous {
                    "anonymous volume".into()
                } else {
                    "named volume: may contain data".into()
                });
                result.items.push(it);
            }
        }
        other => {
            result.status = ProbeStatus::NotImplemented(format!("docker item `{other}`"));
        }
    }
    result
}

fn docker_running(ctx: &ProbeContext) -> ProbeResult {
    let mut r = ProbeResult::empty("docker-containers", None, ProbeStatus::Ok);
    if let Some(data) = ctx.docker() {
        for c in data
            .containers
            .iter()
            .filter(|c| c.status.starts_with("Up"))
        {
            r.items
                .push(ProbeItem::new(c.id.clone(), c.image.clone(), 0));
        }
    }
    r
}

// ------------------------------------------------------------------ Ollama

fn ollama_probe(ctx: &ProbeContext, paths: &[String]) -> ProbeResult {
    let loaded = ctx.ollama_loaded();
    let mut r = ProbeResult::empty("ollama", None, ProbeStatus::Ok);
    if let Some(out) = run("ollama", &["ls"], SHORT) {
        if out.ok() {
            for line in out.stdout.lines().skip(1) {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() < 4 {
                    continue;
                }
                let name = cols[0].to_string();
                let size = parse_docker(&format!("{}{}", cols[2], cols[3])).unwrap_or(0);
                let mut it =
                    ProbeItem::new(name.clone(), name.clone(), size).field("name", name.clone());
                it.in_use = loaded.contains(&name);
                it.note = Some(format!("modified {}", cols[4..].join(" ")));
                r.items.push(it);
            }
            return r;
        }
    }
    // Server down or CLI absent: read the manifests of the store (its own index, R4).
    let store = paths
        .iter()
        .filter_map(|p| ctx.env.expand(p))
        .find(|p| p.join("manifests").is_dir());
    let Some(store) = store else {
        r.status = if installed("ollama") {
            ProbeStatus::Failed("ollama server not running and no model store found".into())
        } else {
            ProbeStatus::ToolMissing("ollama".into())
        };
        return r;
    };
    let manifests = store.join("manifests");
    let mut stack = vec![(manifests.clone(), Vec::<String>::new())];
    while let Some((dir, rel)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let mut rel2 = rel.clone();
            rel2.push(e.file_name().to_string_lossy().to_string());
            if p.is_dir() {
                stack.push((p, rel2));
            } else if rel2.len() >= 4 {
                // registry / namespace / model / tag
                let ns = &rel2[rel2.len() - 3];
                let model = &rel2[rel2.len() - 2];
                let tag = &rel2[rel2.len() - 1];
                let name = if ns == "library" {
                    format!("{model}:{tag}")
                } else {
                    format!("{ns}/{model}:{tag}")
                };
                let size = std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    .map(|v| {
                        v.get("layers")
                            .and_then(|l| l.as_array())
                            .map(|l| {
                                l.iter()
                                    .filter_map(|x| x.get("size").and_then(|s| s.as_u64()))
                                    .sum()
                            })
                            .unwrap_or(0)
                    })
                    .unwrap_or(0);
                let mut it =
                    ProbeItem::new(name.clone(), name.clone(), size).field("name", name.clone());
                it.path = Some(p.clone());
                it.last_used = std::fs::metadata(&p).ok().map(|m| {
                    use std::os::unix::fs::MetadataExt;
                    m.mtime()
                });
                it.note = Some("from the store's manifests (server not running)".into());
                r.items.push(it);
            }
        }
    }
    r.status = ProbeStatus::Ok;
    r
}

// ------------------------------------------------------------ Hugging Face

fn hf_cache_probe(ctx: &ProbeContext, paths: &[String], item_glob: &str) -> ProbeResult {
    let mut r = ProbeResult::empty("hf-cache", Some(item_glob), ProbeStatus::Ok);
    let prefix = item_glob.trim_end_matches('*');
    let Some(root) = paths
        .iter()
        .filter_map(|p| ctx.env.expand(p))
        .find(|p| p.is_dir())
    else {
        return r;
    };
    let Ok(rd) = std::fs::read_dir(&root) else {
        return r;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with(prefix) || !e.path().is_dir() {
            continue;
        }
        let repo = &name[prefix.len()..];
        let (org, model) = match repo.find("--") {
            Some(i) => (repo[..i].to_string(), repo[i + 2..].to_string()),
            None => (String::new(), repo.to_string()),
        };
        let display = if org.is_empty() {
            model.clone()
        } else {
            format!("{org}/{model}")
        };
        let Some((size, mtime, atime)) = ctx.size_of_path(&e.path()) else {
            continue;
        };
        let mut it = ProbeItem::new(name.clone(), display, size)
            .field("org", org)
            .field("name", model);
        it.path = Some(e.path());
        it.last_used = Some(atime.max(mtime));
        r.items.push(it);
    }
    r
}

// ------------------------------------------------------------------- snap

fn snap_probe() -> ProbeResult {
    let mut r = ProbeResult::empty("snap-list-all", None, ProbeStatus::Ok);
    let Some(out) = run("snap", &["list", "--all"], SHORT) else {
        r.status = ProbeStatus::ToolMissing("snap".into());
        return r;
    };
    if !out.ok() {
        r.status = ProbeStatus::Failed(out.stderr.trim().to_string());
        return r;
    }
    for line in out.stdout.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let notes = cols[5..].join(" ");
        if !notes.contains("disabled") {
            continue;
        }
        let (name, rev) = (cols[0], cols[2]);
        let file = PathBuf::from(format!("/var/lib/snapd/snaps/{name}_{rev}.snap"));
        let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        let mut it = ProbeItem::new(
            format!("{name}_{rev}"),
            format!("{name} (revision {rev}, {})", cols[1]),
            size,
        )
        .field("name", name)
        .field("revision", rev);
        it.path = Some(file);
        r.items.push(it);
    }
    r
}

// ---------------------------------------------------------------- journald

fn journal_probe() -> ProbeResult {
    let mut r = ProbeResult::empty("journalctl-disk-usage", None, ProbeStatus::Ok);
    let Some(out) = run("journalctl", &["--disk-usage"], SHORT) else {
        r.status = ProbeStatus::ToolMissing("journalctl".into());
        return r;
    };
    // "Archived and active journals take up 1.3G in the file system."
    let size = out
        .stdout
        .split_whitespace()
        .skip_while(|w| *w != "up")
        .nth(1)
        .and_then(parse_size)
        .unwrap_or(0);
    if size > 0 {
        let keep = 200 * 1024 * 1024;
        let mut it = ProbeItem::new("journal", "systemd journal", size);
        it.reclaimable = Some(size.saturating_sub(keep));
        it.path = Some(PathBuf::from("/var/log/journal"));
        r.items.push(it);
    }
    r
}

// --------------------------------------------------------------------- nvm

fn nvm_probe(ctx: &ProbeContext, paths: &[String]) -> ProbeResult {
    let mut r = ProbeResult::empty("nvm-ls", None, ProbeStatus::Ok);
    let Some(dir) = paths
        .iter()
        .filter_map(|p| ctx.env.expand(p))
        .find(|p| p.is_dir())
    else {
        return r;
    };
    let nvm_dir = dir.parent().and_then(|p| p.parent()).map(Path::to_path_buf);
    let default_alias = nvm_dir
        .as_ref()
        .and_then(|d| std::fs::read_to_string(d.join("alias/default")).ok())
        .map(|s| s.trim().to_string());
    let active = run("node", &["--version"], SHORT).map(|o| o.stdout.trim().to_string());
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return r;
    };
    for e in rd.flatten() {
        let version = e.file_name().to_string_lossy().to_string();
        if Some(&version) == active.as_ref() || Some(&version) == default_alias.as_ref() {
            continue;
        }
        let Some((size, mtime, atime)) = ctx.size_of_path(&e.path()) else {
            continue;
        };
        let globals: Vec<String> = std::fs::read_dir(e.path().join("lib/node_modules"))
            .map(|rd| {
                rd.flatten()
                    .map(|g| g.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let mut it = ProbeItem::new(version.clone(), format!("Node {version}"), size)
            .field("version", version);
        it.path = Some(e.path());
        it.last_used = Some(atime.max(mtime));
        it.note = Some(format!(
            "global packages: {}",
            if globals.is_empty() {
                "none".into()
            } else {
                globals.join(", ")
            }
        ));
        r.items.push(it);
    }
    r
}

// --------------------------------------------------------------------- git

fn git_probe(ctx: &ProbeContext, roots: &[String], exclude_under: &[String]) -> ProbeResult {
    let mut r = ProbeResult::empty("git-status", None, ProbeStatus::Ok);
    if !installed("git") {
        r.status = ProbeStatus::ToolMissing("git".into());
        return r;
    }
    let roots: Vec<PathBuf> = if roots.is_empty() {
        vec![ctx.env.home.clone()]
    } else {
        roots.iter().filter_map(|p| ctx.env.expand(p)).collect()
    };
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in rd.flatten() {
            let dir = e.path();
            if !dir.is_dir() || !dir.join(".git").exists() {
                continue;
            }
            if exclude_under
                .iter()
                .any(|f| crate::paths::contains_fragment(&dir, f))
            {
                continue;
            }
            let d = dir.to_string_lossy().to_string();
            let remote = run("git", &["-C", &d, "remote", "get-url", "origin"], SHORT)
                .filter(|o| o.ok())
                .map(|o| o.stdout.trim().to_string())
                .unwrap_or_default();
            let remote_is_url = remote.starts_with("http://")
                || remote.starts_with("https://")
                || remote.starts_with("git@")
                || remote.starts_with("ssh://");
            if !remote_is_url {
                continue;
            }
            let dirty = run(
                "git",
                &[
                    "-C",
                    &d,
                    "status",
                    "--porcelain",
                    "--untracked-files=normal",
                ],
                LONG,
            )
            .map(|o| !o.ok() || !o.stdout.trim().is_empty())
            .unwrap_or(true);
            if dirty {
                continue;
            }
            let ahead = run(
                "git",
                &["-C", &d, "rev-list", "--count", "@{u}..HEAD"],
                SHORT,
            )
            .map(|o| !o.ok() || o.stdout.trim() != "0")
            .unwrap_or(true);
            if ahead {
                continue;
            }
            let stash = run("git", &["-C", &d, "stash", "list"], SHORT)
                .map(|o| !o.stdout.trim().is_empty())
                .unwrap_or(true);
            if stash {
                continue;
            }
            let Some((size, mtime, atime)) = ctx.size_of_path(&dir) else {
                continue;
            };
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or(d.clone());
            let mut it = ProbeItem::new(d.clone(), name, size)
                .field("remote", remote.clone())
                .field("path", d.clone());
            it.path = Some(dir.clone());
            it.last_used = Some(atime.max(mtime));
            it.note = Some(format!(
                "clean, nothing unpushed; clone again with: git clone {remote}"
            ));
            r.items.push(it);
        }
    }
    r
}

// ------------------------------------------------------------------- conda

fn conda_envs_probe(ctx: &ProbeContext) -> ProbeResult {
    let mut r = ProbeResult::empty("conda-env-list", None, ProbeStatus::Ok);
    let Some(out) = run("conda", &["env", "list", "--json"], LONG) else {
        r.status = ProbeStatus::ToolMissing("conda".into());
        return r;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&out.stdout) else {
        r.status = ProbeStatus::Failed("unparsable `conda env list --json`".into());
        return r;
    };
    let envs: Vec<PathBuf> = v
        .get("envs")
        .and_then(|e| e.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default();
    for env in envs.iter().filter(|p| {
        p.join("conda-meta").is_dir() && p.parent().is_some_and(|q| q.ends_with("envs"))
    }) {
        let Some((size, mtime, atime)) = ctx.size_of_path(env) else {
            continue;
        };
        let name = env
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut it = ProbeItem::new(env.to_string_lossy().to_string(), name.clone(), size)
            .field("name", name);
        it.path = Some(env.clone());
        it.last_used = Some(atime.max(mtime));
        r.items.push(it);
    }
    r
}

fn conda_pkgs_probe(ctx: &ProbeContext) -> ProbeResult {
    let mut r = ProbeResult::empty("conda-pkgs-dirs", None, ProbeStatus::Ok);
    let Some(out) = run("conda", &["info", "--json"], LONG) else {
        r.status = ProbeStatus::ToolMissing("conda".into());
        return r;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&out.stdout) else {
        r.status = ProbeStatus::Failed("unparsable `conda info --json`".into());
        return r;
    };
    for dir in v
        .get("pkgs_dirs")
        .and_then(|e| e.as_array())
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str())
    {
        let p = PathBuf::from(dir);
        let Some((size, mtime, atime)) = ctx.size_of_path(&p) else {
            continue;
        };
        let mut it = ProbeItem::new(dir.to_string(), format!("conda packages ({dir})"), size);
        it.path = Some(p);
        it.last_used = Some(atime.max(mtime));
        r.items.push(it);
    }
    r
}

// -------------------------------------------------------------------- pnpm

fn pnpm_probe(ctx: &ProbeContext) -> ProbeResult {
    let mut r = ProbeResult::empty("pnpm-store-path", None, ProbeStatus::Ok);
    let Some(out) = run("pnpm", &["store", "path"], SHORT) else {
        r.status = ProbeStatus::ToolMissing("pnpm".into());
        return r;
    };
    let path = PathBuf::from(out.stdout.trim());
    if let Some((size, mtime, atime)) = ctx.size_of_path(&path) {
        let mut it = ProbeItem::new("store", format!("pnpm store ({})", path.display()), size);
        it.path = Some(path);
        it.last_used = Some(atime.max(mtime));
        it.note =
            Some("packages are linked into projects; only unreferenced ones are freed".into());
        r.items.push(it);
    }
    r
}

// --------------------------------------------------------------------- apt

fn apt_autoremove_probe() -> ProbeResult {
    let mut r = ProbeResult::empty("apt-autoremove-dry-run", None, ProbeStatus::Ok);
    let Some(out) = run("apt-get", &["-s", "autoremove"], LONG) else {
        r.status = ProbeStatus::ToolMissing("apt-get".into());
        return r;
    };
    let packages: Vec<String> = out
        .stdout
        .lines()
        .filter_map(|l| l.strip_prefix("Remv "))
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();
    if !packages.is_empty() {
        let mut it = ProbeItem::new(
            "autoremove",
            format!("{} packages no longer needed", packages.len()),
            0,
        );
        it.note = Some(packages.join(", "));
        r.items.push(it);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_dates_and_hf_names() {
        assert!(parse_docker_date("2026-10-13 10:22:41 +0200 CEST").is_some());
        assert!(parse_docker_date("garbage").is_none());
        let env = Env::with_home("/nonexistent");
        let ctx = ProbeContext::new(&env, None);
        let r = hf_cache_probe(&ctx, &["~/.cache/huggingface/hub".into()], "models--*");
        assert_eq!(r.status, ProbeStatus::Ok);
        assert!(r.items.is_empty());
    }

    #[test]
    fn hf_probe_reads_a_fake_hub() {
        let tmp = tempfile::tempdir().unwrap();
        let hub = tmp.path().join(".cache/huggingface/hub");
        std::fs::create_dir_all(hub.join("models--Qwen--Qwen-Image/blobs")).unwrap();
        std::fs::write(
            hub.join("models--Qwen--Qwen-Image/blobs/abc"),
            vec![0u8; 100_000],
        )
        .unwrap();
        std::fs::create_dir_all(hub.join("models--gpt2/blobs")).unwrap();
        std::fs::create_dir_all(hub.join("datasets--glue")).unwrap();
        let env = Env::with_home(tmp.path());
        let ctx = ProbeContext::new(&env, None);
        let r = hf_cache_probe(&ctx, &["~/.cache/huggingface/hub".into()], "models--*");
        let mut names: Vec<String> = r.items.iter().map(|i| i.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["Qwen/Qwen-Image", "gpt2"]);
        let qwen = r
            .items
            .iter()
            .find(|i| i.name == "Qwen/Qwen-Image")
            .unwrap();
        assert_eq!(qwen.fields["org"], "Qwen");
        assert_eq!(qwen.fields["name"], "Qwen-Image");
        assert!(qwen.size >= 100_000);
        let gpt2 = r.items.iter().find(|i| i.name == "gpt2").unwrap();
        assert_eq!(gpt2.fields["org"], "");
    }
}
