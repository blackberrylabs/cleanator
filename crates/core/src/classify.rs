//! The classifier: rules × scan × probes → recommendations.
//!
//! For every rule it finds the items the rule describes, sizes them from the scan,
//! applies the dormancy window and the in-use checks (R6), renders the exact command the
//! script would run, and records why. Items no rule matches are not here: they are
//! `never` by construction (R2).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;

use crate::atime::{AtimeContext, AtimeReport};
use crate::exec::installed;
use crate::inuse::Activity;
use crate::paths::{contains_fragment, expand_braces, shell_quote, Env};
use crate::probes::{run_probe, ProbeContext, ProbeResult, ProbeStatus};
use crate::rules::{CleanupKind, DetectKind, Level, Rule, RuleSet};
use crate::scan::{DirNode, Scan};

const PROJECT_MARKERS: &[&str] = &[
    ".git",
    "package.json",
    "pyproject.toml",
    "Cargo.toml",
    "go.mod",
];

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    /// Unique within a report: `rule-id::key`.
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub category: String,
    /// The rule's level.
    pub level: Level,
    /// The level after conditions (an active project raises `safe` to `review`).
    pub effective_level: Level,
    /// What is shown: a path, an image reference, a model name…
    pub display: String,
    pub path: Option<String>,
    pub size: u64,
    /// Space the cleanup frees (differs from `size` for Docker, shared caches).
    pub reclaimable: u64,
    pub last_used: Option<i64>,
    /// Parent project for pattern rules anchored to one.
    pub project: Option<String>,
    pub group: Option<String>,
    pub why: String,
    pub regenerate: String,
    pub cost: String,
    pub notes: Option<String>,
    /// The exact command the script would run; `None` when nothing can be run
    /// (`never`, tool missing without fallback, unresolved placeholder).
    pub command: Option<String>,
    pub cleanup_kind: CleanupKind,
    /// `user` or `sudo`: which block of the script.
    pub requires: String,
    /// Why the item is in use and therefore not proposed, when it is.
    pub in_use: Option<String>,
    /// Classified, but used within the dormancy window: shown (treemap colour, `--explain`)
    /// and never proposed in the script.
    pub recent: bool,
    /// Why the last-use date cannot be trusted, when it cannot: `noatime` (last use
    /// unknown, never proposed) or the mass-read day (proposed only once that day is older
    /// than the window). Such items are also `recent`.
    pub atime_caveat: Option<AtimeCaveat>,
    pub source_url: String,
    pub source_to_verify: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AtimeCaveat {
    /// Filesystem mounted `noatime`: reads are never recorded.
    Noatime,
    /// Last read on the day most of the disk was read at once.
    MassRead,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Totals {
    pub safe: u64,
    pub review: u64,
    pub never: u64,
    pub in_use: u64,
    pub counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub cleanator_version: String,
    pub generated_at: String,
    pub root: PathBuf,
    pub scan: crate::scan::ScanSummary,
    pub rules_origin: String,
    pub rule_count: usize,
    pub dormancy_days: u32,
    pub totals: Totals,
    pub recommendations: Vec<Recommendation>,
    /// Probe statuses, so "not scanned" is explicit.
    pub probes: Vec<ProbeSummary>,
    /// Rules that produced nothing and why (for `--explain`).
    pub rule_notes: Vec<String>,
    /// Whether access times could be trusted, and what it changed.
    pub atime: AtimeReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeSummary {
    pub probe: String,
    pub item: Option<String>,
    pub status: ProbeStatus,
    pub items: usize,
}

#[derive(Debug, Clone)]
pub struct ClassifyOptions {
    pub dormancy_days: u32,
    pub run_probes: bool,
    pub in_use_checks: bool,
    pub now: i64,
    /// Access-time context (mounts, mass-read day). `None` detects it from the system and
    /// the scan; tests inject one.
    pub atime: Option<AtimeContext>,
}

impl Default for ClassifyOptions {
    fn default() -> Self {
        ClassifyOptions {
            dormancy_days: 60,
            run_probes: true,
            in_use_checks: true,
            now: chrono::Utc::now().timestamp(),
            atime: None,
        }
    }
}

/// A candidate item before it becomes a recommendation.
struct Candidate {
    key: String,
    display: String,
    path: Option<PathBuf>,
    size: u64,
    reclaimable: Option<u64>,
    last_used: Option<i64>,
    project: Option<PathBuf>,
    group: Option<String>,
    fields: BTreeMap<String, String>,
    note: Option<String>,
    in_use: Option<String>,
}

/// Build the report for a scan with a rule set.
pub fn classify(scan: &Scan, rules: &RuleSet, env: &Env, opts: &ClassifyOptions) -> Report {
    let ctx = ProbeContext::new(env, Some(&scan.tree));
    let activity = if opts.in_use_checks {
        let mounts = if opts.run_probes {
            ctx.docker_bind_mounts()
        } else {
            Vec::new()
        };
        Activity::collect(mounts)
    } else {
        Activity::empty()
    };
    let managed = managed_store_paths(rules, env);
    let sudo_for_docker = !in_docker_group();
    let atime_ctx = opts
        .atime
        .clone()
        .unwrap_or_else(|| AtimeContext::detect(scan, opts.now));

    let mut recommendations = Vec::new();
    let mut probe_summaries = Vec::new();
    let mut rule_notes = Vec::new();

    // File order, catch-all rules last; the first rule to classify a path keeps it.
    let mut ordered: Vec<&Rule> = rules.rules.iter().collect();
    ordered.sort_by_key(|r| r.is_catch_all());
    let mut claimed: Vec<PathBuf> = Vec::new();

    for rule in ordered {
        let (candidates, probe) = match rule.detect.kind {
            DetectKind::Path => (path_candidates(rule, scan, env, &ctx), None),
            DetectKind::Pattern => (pattern_candidates(rule, scan, env), None),
            DetectKind::Signature => (signature_candidates(rule, scan, &managed), None),
            DetectKind::Probe => {
                let name = rule.detect.probe.as_deref().unwrap_or("");
                if !opts.run_probes && !crate::probes::probe_is_filesystem_only(name) {
                    rule_notes.push(format!("{}: probes disabled", rule.id));
                    continue;
                }
                let pr = run_probe(
                    &ctx,
                    rule.detect.probe.as_deref().unwrap_or(""),
                    rule.detect.item.as_deref(),
                    &rule.detect.paths,
                    &rule.detect.roots,
                    rule.detect.item_glob.as_deref(),
                    &rule.detect.exclude_under,
                );
                let cands = probe_candidates(rule, &pr);
                (cands, Some(pr))
            }
        };
        if let Some(pr) = probe {
            probe_summaries.push(ProbeSummary {
                probe: pr.probe.clone(),
                item: pr.item.clone(),
                status: pr.status.clone(),
                items: pr.items.len(),
            });
        }
        if candidates.is_empty() {
            rule_notes.push(format!("{}: nothing found", rule.id));
            continue;
        }
        let mut seen_keys = HashSet::new();
        let mut newly_claimed: Vec<PathBuf> = Vec::new();
        for cand in candidates {
            if !seen_keys.insert(cand.key.clone()) {
                continue;
            }
            if let Some(p) = &cand.path {
                if claimed.iter().any(|c| p == c || p.starts_with(c)) {
                    continue;
                }
                newly_claimed.push(p.clone());
            }
            if let Some(rec) = finish(
                rule,
                cand,
                scan,
                &activity,
                opts,
                &atime_ctx,
                sudo_for_docker,
            ) {
                recommendations.push(rec);
            }
        }
        claimed.extend(newly_claimed);
    }

    recommendations.sort_by(|a, b| {
        a.effective_level
            .cmp(&b.effective_level)
            .then_with(|| a.recent.cmp(&b.recent))
            .then_with(|| a.in_use.is_some().cmp(&b.in_use.is_some()))
            .then_with(|| b.reclaimable.cmp(&a.reclaimable))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut totals = Totals::default();
    for r in &recommendations {
        if r.recent {
            *totals.counts.entry("recent".to_string()).or_default() += 1;
            continue;
        }
        *totals
            .counts
            .entry(r.effective_level.to_string())
            .or_default() += 1;
        if r.in_use.is_some() {
            totals.in_use += r.reclaimable;
            continue;
        }
        match r.effective_level {
            Level::Safe => totals.safe += r.reclaimable,
            Level::Review => totals.review += r.reclaimable,
            Level::Never => totals.never += r.size,
        }
    }

    let mut atime = AtimeReport {
        mass_read: atime_ctx.mass_read,
        ..Default::default()
    };
    let mut noatime_mounts = BTreeSet::new();
    for r in &recommendations {
        match r.atime_caveat {
            Some(AtimeCaveat::Noatime) => {
                atime.unknown_items += 1;
                atime.unknown_size += r.size;
                if let Some(p) = &r.path {
                    if let Some(m) = atime_ctx.noatime_mount(Path::new(p)) {
                        noatime_mounts.insert(m.to_string_lossy().to_string());
                    }
                }
            }
            Some(AtimeCaveat::MassRead) => {
                atime.deferred_items += 1;
                atime.deferred_size += r.size;
            }
            None => {}
        }
    }
    atime.noatime_mounts = noatime_mounts.into_iter().collect();
    if atime.deferred_items > 0 {
        atime.deferred_until = atime_ctx
            .mass_read
            .map(|m| m.day + opts.dormancy_days as i64 * 86_400);
    }

    Report {
        cleanator_version: crate::VERSION.to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        root: scan.summary.root.clone(),
        scan: scan.summary.clone(),
        rules_origin: rules.origin.clone(),
        rule_count: rules.len(),
        dormancy_days: opts.dormancy_days,
        totals,
        recommendations,
        probes: probe_summaries,
        rule_notes,
        atime,
    }
}

// ------------------------------------------------------------------ helpers

fn in_docker_group() -> bool {
    crate::exec::run("id", &["-nG"], crate::exec::SHORT)
        .map(|o| o.stdout.split_whitespace().any(|g| g == "docker"))
        .unwrap_or(false)
}

/// Existing directories of every `path`-kind rule and probe hint: a managed store.
fn managed_store_paths(rules: &RuleSet, env: &Env) -> Vec<PathBuf> {
    let mut out = BTreeSet::new();
    for r in &rules.rules {
        if matches!(r.detect.kind, DetectKind::Path | DetectKind::Probe) {
            for t in &r.detect.paths {
                if let Some(p) = env.expand(t) {
                    if p.is_dir() {
                        out.insert(p);
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

fn node_last_used(node: &DirNode) -> i64 {
    node.newest_mtime.max(node.newest_atime)
}

/// Visit every directory with its ancestors (root first).
fn walk<'a>(
    node: &'a DirNode,
    ancestors: &mut Vec<&'a DirNode>,
    f: &mut dyn FnMut(&DirNode, &[&DirNode]),
) {
    f(node, ancestors);
    ancestors.push(node);
    for c in &node.children {
        walk(c, ancestors, f);
    }
    ancestors.pop();
}

fn project_of<'a>(ancestors: &[&'a DirNode]) -> Option<&'a DirNode> {
    ancestors
        .iter()
        .rev()
        .find(|a| PROJECT_MARKERS.iter().any(|m| a.has_marker(m)))
        .copied()
}

fn build_globset(patterns: &[String], env: &Env) -> Option<GlobSet> {
    let mut b = GlobSetBuilder::new();
    let mut any = false;
    for p in patterns {
        for expanded in expand_braces(p) {
            let text = match env.expand(&expanded) {
                Some(path) => path.to_string_lossy().to_string(),
                None => continue,
            };
            if let Ok(g) = Glob::new(&text) {
                b.add(g);
                any = true;
            }
        }
    }
    if any {
        b.build().ok()
    } else {
        None
    }
}

// ------------------------------------------------------------- path rules

fn path_candidates(rule: &Rule, scan: &Scan, env: &Env, ctx: &ProbeContext) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut found_any = false;
    for template in &rule.detect.paths {
        for expanded in expand_braces(template) {
            let Some(path) = env.expand(&expanded) else {
                continue;
            };
            // A template with a wildcard (`/usr/local/cuda-*`) lists matching siblings.
            let matches: Vec<PathBuf> = if path.to_string_lossy().contains('*') {
                glob_siblings(&path)
            } else if path.exists() {
                vec![path]
            } else {
                Vec::new()
            };
            for p in matches {
                found_any = true;
                if let Some(ig) = &rule.detect.item_glob {
                    let Ok(g) = Glob::new(ig).map(|g| g.compile_matcher()) else {
                        continue;
                    };
                    if let Ok(rd) = std::fs::read_dir(&p) {
                        for e in rd.flatten() {
                            let name = e.file_name().to_string_lossy().to_string();
                            if g.is_match(&name) {
                                if let Some(c) = candidate_for_path(&e.path(), scan, ctx) {
                                    out.push(c);
                                }
                            }
                        }
                    }
                } else if let Some(c) = candidate_for_path(&p, scan, ctx) {
                    out.push(c);
                }
            }
        }
        // `paths` are alternatives tried in order: stop at the first that exists,
        // unless the rule lists several distinct locations (then all are reported).
        if found_any && rule.detect.paths.len() <= 3 && rule.detect.item_glob.is_none() {
            break;
        }
    }
    out
}

fn glob_siblings(pattern: &Path) -> Vec<PathBuf> {
    let Some(parent) = pattern.parent() else {
        return Vec::new();
    };
    let Some(name) = pattern.file_name().map(|n| n.to_string_lossy().to_string()) else {
        return Vec::new();
    };
    let Ok(g) = Glob::new(&name).map(|g| g.compile_matcher()) else {
        return Vec::new();
    };
    std::fs::read_dir(parent)
        .map(|rd| {
            rd.flatten()
                .filter(|e| g.is_match(e.file_name().to_string_lossy().as_ref()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

fn candidate_for_path(path: &Path, scan: &Scan, ctx: &ProbeContext) -> Option<Candidate> {
    let (size, mtime, atime) = if let Some(node) = scan.tree.find(path) {
        (node.size, node.newest_mtime, node.newest_atime)
    } else {
        ctx.size_of_path(path)?
    };
    if size == 0 {
        return None;
    }
    let display = path.to_string_lossy().to_string();
    Some(Candidate {
        key: display.clone(),
        display,
        path: Some(path.to_path_buf()),
        size,
        reclaimable: None,
        last_used: Some(mtime.max(atime)),
        project: None,
        group: None,
        fields: BTreeMap::new(),
        note: None,
        in_use: None,
    })
}

// ---------------------------------------------------------- pattern rules

fn pattern_candidates(rule: &Rule, scan: &Scan, env: &Env) -> Vec<Candidate> {
    let d = &rule.detect;
    let globs = build_globset(&d.glob, env);
    let min_size = d.min_size_bytes().unwrap_or(0);
    let mut out: Vec<Candidate> = Vec::new();
    let mut matched_dirs: Vec<PathBuf> = Vec::new();
    let mut ancestors = Vec::new();
    walk(&scan.tree, &mut ancestors, &mut |node, ancestors| {
        if node.other_fs {
            return;
        }
        // directories
        let dir_match = (globs.as_ref().is_some_and(|g| g.is_match(&node.path))
            || d.marker.as_deref().is_some_and(|m| node.has_marker(m)))
            && !is_root(node, scan);
        if dir_match
            && node.size >= min_size
            && !excluded(&node.path, &d.exclude_under)
            && sibling_ok(d, ancestors)
            && !(d.skip_in_project && project_of(ancestors).is_some())
            && !matched_dirs.iter().any(|m| node.path.starts_with(m))
        {
            matched_dirs.push(node.path.clone());
            let project = if d.anchor.as_deref() == Some("project") {
                project_of(ancestors).map(|p| p.path.clone())
            } else {
                None
            };
            out.push(Candidate {
                key: node.path.to_string_lossy().to_string(),
                display: node.path.to_string_lossy().to_string(),
                path: Some(node.path.clone()),
                size: node.size,
                reclaimable: None,
                last_used: Some(node_last_used(node)),
                project,
                group: group_for(d.group_by.as_deref(), &node.path, ancestors),
                fields: BTreeMap::new(),
                note: None,
                in_use: None,
            });
            return;
        }
        // big files inside this directory
        if let Some(g) = &globs {
            for f in &node.big_files {
                if f.size == 0
                    || f.size < min_size
                    || !g.is_match(&f.path)
                    || excluded(&f.path, &d.exclude_under)
                {
                    continue;
                }
                if matched_dirs.iter().any(|m| f.path.starts_with(m)) {
                    continue;
                }
                if d.skip_in_project
                    && (project_of(ancestors).is_some()
                        || PROJECT_MARKERS.iter().any(|m| node.has_marker(m)))
                {
                    continue;
                }
                let project = if d.anchor.as_deref() == Some("project") {
                    project_of(ancestors).map(|p| p.path.clone())
                } else {
                    None
                };
                let mut parents = ancestors.to_vec();
                parents.push(node);
                out.push(Candidate {
                    key: f.path.to_string_lossy().to_string(),
                    display: f.path.to_string_lossy().to_string(),
                    path: Some(f.path.clone()),
                    size: f.size,
                    reclaimable: None,
                    last_used: Some(f.mtime.max(f.atime)),
                    project,
                    group: group_for(d.group_by.as_deref(), &f.path, &parents),
                    fields: BTreeMap::new(),
                    note: None,
                    in_use: None,
                });
            }
        }
    });
    out
}

fn is_root(node: &DirNode, scan: &Scan) -> bool {
    node.path == scan.summary.root
}

/// `exclude_under` applies to what the item is *below*: its ancestors, not itself (a
/// `node_modules` directory is matched; a `node_modules` inside another is not).
fn excluded(path: &Path, fragments: &[String]) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    fragments.iter().any(|f| contains_fragment(parent, f))
}

fn sibling_ok(d: &crate::rules::Detect, ancestors: &[&DirNode]) -> bool {
    if d.requires_sibling.is_empty() {
        return true;
    }
    ancestors
        .last()
        .is_some_and(|parent| d.requires_sibling.iter().any(|m| parent.has_marker(m)))
}

fn group_for(group_by: Option<&str>, path: &Path, ancestors: &[&DirNode]) -> Option<String> {
    match group_by {
        None => None,
        Some("project") => project_of(ancestors).map(|p| p.path.to_string_lossy().to_string()),
        Some("app") => ancestors
            .iter()
            .rev()
            .find(|a| {
                a.path.parent().is_some_and(|p| p.ends_with(".config")) || a.name.starts_with('.')
            })
            .map(|a| a.name.clone()),
        Some(other) => {
            if let Some(n) = other.strip_prefix("parent:") {
                let n: usize = n.parse().unwrap_or(1);
                let mut p = path;
                for _ in 0..n {
                    p = p.parent().unwrap_or(p);
                }
                Some(p.to_string_lossy().to_string())
            } else {
                None
            }
        }
    }
}

// -------------------------------------------------------- signature rules

fn signature_candidates(rule: &Rule, scan: &Scan, managed: &[PathBuf]) -> Vec<Candidate> {
    let d = &rule.detect;
    let min_size = d.min_size_bytes().unwrap_or(0);
    let any = d.extensions.iter().any(|e| e == "*");
    let exts: Vec<String> = d
        .extensions
        .iter()
        .map(|e| format!(".{}", e.to_ascii_lowercase()))
        .collect();
    let mut out = Vec::new();
    let mut ancestors = Vec::new();
    walk(&scan.tree, &mut ancestors, &mut |node, ancestors| {
        if node.other_fs {
            return;
        }
        if d.exclude_managed && managed.iter().any(|m| node.path.starts_with(m)) {
            return;
        }
        for f in &node.big_files {
            if f.size == 0 || f.size < min_size {
                continue;
            }
            let lname = f.name.to_ascii_lowercase();
            if !any && !exts.iter().any(|e| lname.ends_with(e)) {
                continue;
            }
            if excluded(&f.path, &d.exclude_under) {
                continue;
            }
            let mut parents = ancestors.to_vec();
            parents.push(node);
            out.push(Candidate {
                key: f.path.to_string_lossy().to_string(),
                display: f.path.to_string_lossy().to_string(),
                path: Some(f.path.clone()),
                size: f.size,
                reclaimable: None,
                last_used: Some(f.mtime.max(f.atime)),
                project: project_of(ancestors).map(|p| p.path.clone()),
                group: Some(node.path.to_string_lossy().to_string()),
                fields: BTreeMap::new(),
                note: if f.nlink > 1 {
                    Some(format!("{} hard links", f.nlink))
                } else {
                    None
                },
                in_use: None,
            });
        }
    });
    out
}

// ------------------------------------------------------------ probe rules

fn probe_candidates(rule: &Rule, pr: &ProbeResult) -> Vec<Candidate> {
    let min_size = rule.detect.min_size_bytes().unwrap_or(0);
    pr.items
        .iter()
        .filter(|i| i.size >= min_size)
        .map(|i| Candidate {
            key: i.key.clone(),
            display: i.name.clone(),
            path: i.path.clone(),
            size: i.size,
            reclaimable: i.reclaimable,
            last_used: i.last_used,
            project: None,
            group: None,
            fields: i.fields.clone(),
            note: i.note.clone(),
            in_use: if i.in_use {
                Some("loaded or running".into())
            } else {
                None
            },
        })
        .collect()
}

// -------------------------------------------------------------- finishing

fn finish(
    rule: &Rule,
    mut cand: Candidate,
    scan: &Scan,
    activity: &Activity,
    opts: &ClassifyOptions,
    atime_ctx: &AtimeContext,
    sudo_for_docker: bool,
) -> Option<Recommendation> {
    let cond = rule.conditions.clone().unwrap_or_default();
    let window = rule
        .dormancy_days(opts.dormancy_days)
        .map(|d| d as i64 * 86_400);
    let mut effective = rule.level;
    let mut notes: Vec<String> = Vec::new();
    if let Some(n) = cand.note.take() {
        notes.push(n);
    }

    // Last use according to the rule's measure.
    let mut atime_caveat: Option<AtimeCaveat> = None;
    let measured_by_atime = cond
        .dormancy_measure
        .as_deref()
        .is_some_and(|m| m.starts_with("atime"));
    if let Some(measure) = cond.dormancy_measure.as_deref() {
        if let Some(rel) = measure.strip_prefix("atime:") {
            if let Some(p) = &cand.path {
                if let Ok(m) = std::fs::metadata(p.join(rel)) {
                    use std::os::unix::fs::MetadataExt;
                    cand.last_used = Some(m.atime().max(m.mtime()));
                }
            }
        } else if measure == "mtime" {
            if let Some(p) = &cand.path {
                if let Some(node) = scan.tree.find(p) {
                    cand.last_used = Some(node.newest_mtime);
                }
            }
        } else if measure == "atime" {
            if let Some(p) = &cand.path {
                if let Some(node) = scan.tree.find(p) {
                    cand.last_used = Some(node.newest_atime);
                } else if let Ok(m) = std::fs::metadata(p) {
                    use std::os::unix::fs::MetadataExt;
                    cand.last_used = Some(m.atime());
                }
            }
        }
    }

    // Can the access time be trusted? (D1–D3 of the 0.1.1 design.)
    if measured_by_atime && rule.level == Level::Review {
        if let Some(p) = &cand.path {
            if let Some(mount) = atime_ctx.noatime_mount(p) {
                // Reads are never recorded here: the last use is unknown, and unknown is
                // not proposed (R2). The date shown would be the copy date; drop it.
                atime_caveat = Some(AtimeCaveat::Noatime);
                cand.last_used = None;
                notes.push(format!(
                    "last use unknown: {} is mounted noatime, so reads are never recorded \
                     — shown, not proposed",
                    mount.display()
                ));
            }
        }
        if atime_caveat.is_none() {
            if let (Some(mass), Some(t), Some(window)) =
                (atime_ctx.mass_read, cand.last_used, window)
            {
                if mass.covers(t) && opts.now - mass.day < window {
                    // Read along with most of the disk that day: no evidence of use since
                    // then, nor of non-use before. Dormant only once that day is old enough.
                    atime_caveat = Some(AtimeCaveat::MassRead);
                    notes.push(format!(
                        "last read on {}, the day a large part of this disk was read at once \
                         ({} of {} large files); real last use unknown — proposed from {} \
                         unless used",
                        crate::size::ymd(mass.day),
                        mass.files,
                        mass.total,
                        crate::size::ymd(mass.day + window)
                    ));
                }
            }
        }
    }

    // Dormancy: review rules skip recent items; safe rules raise their level on active
    // projects when they say so.
    let mut recent_item = atime_caveat == Some(AtimeCaveat::Noatime);
    if let Some(window) = window {
        let recent = cand.last_used.is_some_and(|t| opts.now - t < window);
        match rule.level {
            Level::Review if recent => recent_item = true,
            Level::Safe => {
                if let (Some(active_level), Some(project)) =
                    (cond.active_project_level, &cand.project)
                {
                    let project_recent = scan
                        .tree
                        .find(project)
                        .is_some_and(|n| opts.now - n.newest_source_mtime < window);
                    if project_recent {
                        effective = active_level.max(rule.level);
                        notes.push(format!(
                            "project active (touched within {} days): kept for review",
                            window / 86_400
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    // In-use checks (R6).
    let mut in_use = cand.in_use.take();
    if in_use.is_none() && rule.level != Level::Never {
        if let Some(p) = &cand.path {
            if rule.checks_bind_mounts() {
                if let Some(m) = activity.bind_mounted(p) {
                    in_use = Some(format!(
                        "mounted into a running container ({})",
                        m.display()
                    ));
                }
            }
            if in_use.is_none() && rule.checks_open_files() && activity.path_open(p) {
                in_use = Some("a running process has files open here".into());
            }
        }
        if in_use.is_none() {
            if let Some(u) = &rule.in_use {
                // Without a path there is nothing precise to check: fall back to
                // "is one of these processes running".
                let coarse = !u.processes.is_empty() && u.probe.is_none() && cand.path.is_none();
                if coarse {
                    if let Some(name) = activity.process_running(&u.processes) {
                        in_use = Some(format!("`{name}` is running"));
                    }
                }
            }
        }
    }

    // Command.
    let (command, requires, cmd_note) = render_command(rule, &cand, effective, sudo_for_docker);
    if let Some(n) = cmd_note {
        notes.push(n);
    }
    if let Some(n) = &rule.explain.notes {
        notes.push(n.clone());
    }

    let reclaimable = cand.reclaimable.unwrap_or(cand.size);
    Some(Recommendation {
        id: format!("{}::{}", rule.id, cand.key),
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        category: rule.category.clone(),
        level: rule.level,
        effective_level: effective,
        display: cand.display,
        path: cand.path.map(|p| p.to_string_lossy().to_string()),
        size: cand.size,
        reclaimable,
        last_used: cand.last_used,
        project: cand.project.map(|p| p.to_string_lossy().to_string()),
        group: cand.group,
        why: rule.explain.why.clone(),
        regenerate: rule.explain.regenerate.clone(),
        cost: rule.explain.cost.clone(),
        notes: if notes.is_empty() {
            None
        } else {
            Some(notes.join(" · "))
        },
        command,
        cleanup_kind: rule.cleanup.kind,
        requires,
        in_use,
        recent: recent_item,
        atime_caveat,
        source_url: rule.source.url.clone(),
        source_to_verify: rule.source_to_verify(),
    })
}

/// The exact command for an item, the block it belongs to, and a note when the
/// cleanup had to change (tool missing → fallback) or cannot be rendered.
fn render_command(
    rule: &Rule,
    cand: &Candidate,
    level: Level,
    sudo_for_docker: bool,
) -> (Option<String>, String, Option<String>) {
    let requires = match rule.cleanup.requires.as_deref() {
        Some("sudo") => "sudo",
        Some("docker-group-or-sudo") if sudo_for_docker => "sudo",
        _ => "user",
    }
    .to_string();
    if level == Level::Never || rule.cleanup.kind == CleanupKind::None {
        return (None, requires, None);
    }
    let path_q = cand
        .path
        .as_ref()
        .map(|p| shell_quote(&p.to_string_lossy()));
    let rm = path_q.as_ref().map(|q| format!("rm -rf -- {q}"));
    let trash = path_q.as_ref().map(|q| format!("gio trash -- {q}"));
    // A `safe` item is removed; a `review` item goes to the trash (R13), unless the
    // rule names its tool's command.
    let kind = rule.cleanup.kind;
    match kind {
        CleanupKind::Rm => {
            if level == Level::Review {
                (
                    trash,
                    requires,
                    Some("moved to the trash because it needs review".into()),
                )
            } else {
                (rm, requires, None)
            }
        }
        CleanupKind::Trash => (trash, requires, None),
        CleanupKind::Command => {
            let template = rule.cleanup.command.clone().unwrap_or_default();
            let tool = template
                .split_whitespace()
                .find(|w| *w != "sudo")
                .unwrap_or("");
            if !tool.is_empty() && !installed(tool) {
                return match rule.cleanup.fallback.as_deref() {
                    Some("rm") if level == Level::Safe => (
                        rm,
                        requires,
                        Some(format!("`{tool}` not installed: plain removal instead")),
                    ),
                    Some("rm") | Some("trash") => (
                        trash,
                        requires,
                        Some(format!(
                            "`{tool}` not installed: moved to the trash instead"
                        )),
                    ),
                    _ => (
                        None,
                        requires,
                        Some(format!(
                            "`{tool}` not installed and no safe fallback: nothing generated"
                        )),
                    ),
                };
            }
            match substitute(&template, cand) {
                Some(c) => {
                    let c = if requires == "sudo" && !c.starts_with("sudo ") {
                        format!("sudo {c}")
                    } else {
                        c
                    };
                    (Some(c), requires, None)
                }
                None => (
                    None,
                    requires,
                    Some("command has a placeholder this item cannot fill".into()),
                ),
            }
        }
        CleanupKind::None => (None, requires, None),
    }
}

fn substitute(template: &str, cand: &Candidate) -> Option<String> {
    let mut out = template.to_string();
    if cand.fields.get("org").is_some_and(|o| o.is_empty()) {
        out = out.replace("{org}/", "");
    }
    let mut fields = cand.fields.clone();
    if let Some(p) = &cand.path {
        fields
            .entry("path".into())
            .or_insert(p.to_string_lossy().to_string());
    }
    if let Some(p) = &cand.project {
        fields
            .entry("project".into())
            .or_insert(p.to_string_lossy().to_string());
    }
    for (k, v) in &fields {
        let needs_quote = k == "path" || k == "project";
        let value = if needs_quote {
            shell_quote(v)
        } else {
            v.clone()
        };
        out = out.replace(&format!("{{{k}}}"), &value);
    }
    if out.contains('{') && out.contains('}') {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{scan, ScanOptions};
    use std::fs;
    use std::io::Write;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(path).unwrap();
        f.write_all(&vec![b'x'; bytes]).unwrap();
    }

    fn age(path: &Path, days: i64) {
        let secs = chrono::Utc::now().timestamp() - days * 86_400;
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
        let f = fs::File::open(path).unwrap();
        f.set_modified(t).unwrap();
        f.set_times(fs::FileTimes::new().set_accessed(t).set_modified(t))
            .unwrap();
    }

    /// A fake home with one case per level, an active and a dormant project, a venv at the
    /// home root, a hard-linked model and a loose safetensors.
    fn fake_home() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let h = tmp.path();
        write(&h.join(".cache/pip/http/a"), 3_000_000);
        write(&h.join("active/package.json"), 10);
        write(&h.join("active/src/index.js"), 10);
        write(&h.join("active/node_modules/x/big.js"), 2_000_000);
        write(&h.join("dormant/package.json"), 10);
        write(&h.join("dormant/src/index.js"), 10);
        write(&h.join("dormant/node_modules/y/big.js"), 2_000_000);
        write(&h.join("venvs/old/pyvenv.cfg"), 10);
        write(&h.join("venvs/old/bin/python"), 1_500_000);
        write(&h.join("venvs/fresh/pyvenv.cfg"), 10);
        write(&h.join("venvs/fresh/bin/python"), 1_500_000);
        write(&h.join("comfy/models/qwen.safetensors"), 101 * 1024 * 1024);
        write(&h.join("comfy/models/fresh.safetensors"), 101 * 1024 * 1024);
        // a backup folder inside a project: the project's business, not ours
        write(
            &h.join("active/corpus/_backups/dump.sql"),
            101 * 1024 * 1024,
        );
        // a backup archive matched by two rules (backup name + .tar.gz): listed once
        write(
            &h.join("claude_config_backup_20260703.tar.gz"),
            101 * 1024 * 1024,
        );
        write(&h.join(".claude/settings.json"), 10);
        write(&h.join(".config/Claude_backup/x.img"), 101 * 1024 * 1024);
        for p in [
            "comfy/models/qwen.safetensors",
            "claude_config_backup_20260703.tar.gz",
            "dormant/src/index.js",
            "dormant/package.json",
            "dormant/src",
            "dormant",
            "venvs/old/bin/python",
            "venvs/old/pyvenv.cfg",
            "venvs/old/bin",
            "venvs/old",
        ] {
            age(&h.join(p), 120);
        }
        tmp
    }

    /// `qwen.safetensors` is 120 days old (dormant), `fresh.safetensors` was read today.
    fn report_with_atime(home: &Path, env: &Env, ctx: AtimeContext) -> Report {
        let scan = scan(
            home,
            ScanOptions {
                big_file_threshold: 1_000_000,
                one_filesystem: true,
                progress: None,
            },
        )
        .unwrap();
        let rules = RuleSet::embedded().unwrap();
        let copts = ClassifyOptions {
            dormancy_days: 60,
            run_probes: false,
            in_use_checks: false,
            atime: Some(ctx),
            ..Default::default()
        };
        classify(&scan, &rules, env, &copts)
    }

    #[test]
    fn noatime_mount_makes_last_use_unknown_and_never_proposed() {
        let tmp = fake_home();
        let home = tmp.path();
        let env = Env::with_home(home);
        let mounts = crate::atime::parse_mounts(&format!(
            "/dev/a / ext4 rw,relatime 0 0\n/dev/b {} ext4 rw,noatime 0 0\n",
            home.display()
        ));
        let report = report_with_atime(
            home,
            &env,
            AtimeContext {
                mounts,
                mass_read: None,
            },
        );
        let loose: Vec<_> = report
            .recommendations
            .iter()
            .filter(|r| r.rule_id == "loose-model-files")
            .collect();
        assert_eq!(loose.len(), 2, "still classified: {loose:?}");
        for r in &loose {
            assert_eq!(r.atime_caveat, Some(AtimeCaveat::Noatime));
            assert!(r.recent, "never proposed");
            assert_eq!(r.last_used, None, "no misleading copy date");
            assert!(r.notes.as_deref().unwrap().contains("noatime"));
        }
        assert_eq!(
            report.atime.unknown_items,
            loose.len() + 2,
            "models + both venvs"
        );
        assert_eq!(
            report.atime.noatime_mounts,
            vec![home.to_string_lossy().to_string()]
        );
        // mtime-measured rules are untouched
        assert!(report
            .recommendations
            .iter()
            .any(|r| r.rule_id == "node-modules" && !r.recent));
    }

    #[test]
    fn mass_read_day_defers_instead_of_dating() {
        let tmp = fake_home();
        let home = tmp.path();
        let env = Env::with_home(home);
        // Everything was "read" 30 days ago by an indexer: the dormant model's atime too.
        let now = chrono::Utc::now().timestamp();
        let indexer_day = now - 30 * 86_400;
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(indexer_day as u64);
        let qwen = home.join("comfy/models/qwen.safetensors");
        fs::File::open(&qwen)
            .unwrap()
            .set_times(fs::FileTimes::new().set_accessed(t))
            .unwrap();
        let mass = crate::atime::MassRead {
            day: crate::atime::day_of(indexer_day),
            files: 400,
            total: 500,
        };
        let report = report_with_atime(
            home,
            &env,
            AtimeContext {
                mounts: Vec::new(),
                mass_read: Some(mass),
            },
        );
        let q = report
            .recommendations
            .iter()
            .find(|r| r.path.as_deref() == Some(qwen.to_str().unwrap()))
            .expect("qwen classified");
        assert_eq!(q.atime_caveat, Some(AtimeCaveat::MassRead));
        assert!(
            q.recent,
            "not proposed until the mass-read day is 60 days old"
        );
        let note = q.notes.as_deref().unwrap();
        assert!(
            note.contains("a large part of this disk was read"),
            "{note}"
        );
        assert!(note.contains(&crate::size::ymd(mass.day + 60 * 86_400)));
        assert_eq!(report.atime.deferred_items, 1);
        assert_eq!(report.atime.deferred_until, Some(mass.day + 60 * 86_400));
        // A mass read older than the window changes nothing: the item is simply dormant.
        let old = crate::atime::MassRead {
            day: crate::atime::day_of(now - 90 * 86_400),
            ..mass
        };
        fs::File::open(&qwen)
            .unwrap()
            .set_times(fs::FileTimes::new().set_accessed(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(old.day as u64 + 10),
            ))
            .unwrap();
        let report = report_with_atime(
            home,
            &env,
            AtimeContext {
                mounts: Vec::new(),
                mass_read: Some(old),
            },
        );
        let q = report
            .recommendations
            .iter()
            .find(|r| r.path.as_deref() == Some(qwen.to_str().unwrap()))
            .unwrap();
        assert_eq!(q.atime_caveat, None);
        assert!(!q.recent, "dormant and proposed");
    }

    #[test]
    fn detects_a_mass_read_day_in_a_scan() {
        let tmp = tempfile::tempdir().unwrap();
        let h = tmp.path();
        let now = chrono::Utc::now().timestamp();
        let day = crate::atime::day_of(now - 20 * 86_400);
        for i in 0..250 {
            let p = h.join(format!("data/f{i}.bin"));
            write(&p, 1_000_001);
            let secs = if i < 220 {
                day + 3600
            } else {
                now - 5 * 86_400
            };
            fs::File::open(&p)
                .unwrap()
                .set_times(fs::FileTimes::new().set_accessed(
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64),
                ))
                .unwrap();
        }
        // build directories do not count
        for i in 0..100 {
            let p = h.join(format!("proj/node_modules/g{i}.bin"));
            write(&p, 1_000_001);
        }
        let scan = scan(
            h,
            ScanOptions {
                big_file_threshold: 1_000_000,
                one_filesystem: true,
                progress: None,
            },
        )
        .unwrap();
        let m = crate::atime::detect_mass_read(&scan, now).expect("mass read detected");
        assert_eq!(m.day, day);
        assert_eq!(m.files, 220);
        assert_eq!(m.total, 250);
    }

    #[test]
    fn classifies_one_case_per_level() {
        let tmp = fake_home();
        let home = tmp.path();
        let env = Env::with_home(home);
        let opts = ScanOptions {
            big_file_threshold: 1_000_000,
            one_filesystem: true,
            progress: None,
        };
        let scan = scan(home, opts).unwrap();
        let rules = RuleSet::embedded().unwrap();
        let copts = ClassifyOptions {
            dormancy_days: 60,
            run_probes: false,
            in_use_checks: false,
            ..Default::default()
        };
        let report = classify(&scan, &rules, &env, &copts);

        let by_rule = |id: &str| -> Vec<&Recommendation> {
            report
                .recommendations
                .iter()
                .filter(|r| r.rule_id == id)
                .collect()
        };

        let pip = by_rule("pip-cache");
        assert_eq!(pip.len(), 1, "pip cache found once");
        assert_eq!(pip[0].effective_level, Level::Safe);
        assert!(pip[0]
            .command
            .as_deref()
            .is_some_and(|c| c.contains("pip cache purge") || c.starts_with("rm -rf")));

        let nm = by_rule("node-modules");
        assert_eq!(nm.len(), 2, "two node_modules");
        let active = nm
            .iter()
            .find(|r| r.path.as_deref().unwrap().contains("/active/"))
            .unwrap();
        let dormant = nm
            .iter()
            .find(|r| r.path.as_deref().unwrap().contains("/dormant/"))
            .unwrap();
        assert_eq!(
            active.effective_level,
            Level::Review,
            "active project raises to review"
        );
        assert_eq!(dormant.effective_level, Level::Safe);
        assert!(dormant
            .command
            .as_deref()
            .unwrap()
            .starts_with("rm -rf -- '"));
        assert!(active
            .command
            .as_deref()
            .unwrap()
            .starts_with("gio trash -- '"));
        assert_eq!(
            dormant.project.as_deref(),
            Some(home.join("dormant").to_str().unwrap())
        );

        let venvs: Vec<_> = by_rule("python-venv")
            .into_iter()
            .filter(|r| !r.recent)
            .collect();
        assert_eq!(
            venvs.len(),
            1,
            "only the dormant venv is proposed: {venvs:?}"
        );
        assert!(venvs[0].path.as_deref().unwrap().ends_with("venvs/old"));

        let loose = by_rule("loose-model-files");
        assert_eq!(loose.len(), 2, "both models are classified: {loose:?}");
        let proposed: Vec<_> = loose.iter().filter(|r| !r.recent).collect();
        assert_eq!(
            proposed.len(),
            1,
            "the model loaded today is classified but not proposed"
        );
        assert!(proposed[0].display.ends_with("qwen.safetensors"));
        assert!(loose
            .iter()
            .any(|r| r.recent && r.display.ends_with("fresh.safetensors")));
        assert_eq!(proposed[0].effective_level, Level::Review);

        // one owner per path: the archive is a backup first, never an "archive" too, and
        // the catch-all never re-lists classified files
        let tgz: Vec<_> = report
            .recommendations
            .iter()
            .filter(|r| r.display.ends_with("claude_config_backup_20260703.tar.gz"))
            .collect();
        assert_eq!(tgz.len(), 1, "{tgz:?}");
        assert_eq!(tgz[0].rule_id, "backup-directories");
        let catch_all = by_rule("large-files-unclassified");
        assert!(
            catch_all
                .iter()
                .all(|r| !r.display.ends_with(".safetensors") && !r.display.ends_with(".tar.gz")),
            "{catch_all:?}"
        );
        assert!(
            by_rule("backup-directories")
                .iter()
                .all(|r| !r.display.contains("/active/")),
            "a backup folder inside a project is skipped"
        );

        let keep = by_rule("claude-code-keep");
        assert!(!keep.is_empty());
        assert!(keep
            .iter()
            .all(|r| r.command.is_none() && r.effective_level == Level::Never));

        let backup = by_rule("backup-directories");
        assert_eq!(
            backup.len(),
            2,
            "the Claude_backup dir and the .tar.gz: {backup:?}"
        );

        assert!(report.totals.safe > 0 && report.totals.review > 0);
        let ids: HashSet<&str> = report
            .recommendations
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(ids.len(), report.recommendations.len(), "ids are unique");
    }

    #[test]
    fn in_use_items_are_flagged_not_dropped() {
        let tmp = fake_home();
        let home = tmp.path();
        let env = Env::with_home(home);
        let scan = scan(
            home,
            ScanOptions {
                big_file_threshold: 1_000_000,
                one_filesystem: true,
                progress: None,
            },
        )
        .unwrap();
        let rules = RuleSet::embedded().unwrap();
        // pretend the dormant venv's python is running: its file is "open"
        let activity = Activity::for_test(vec![home.join("venvs/old/bin/python")]);
        let rule = rules.get("python-venv").unwrap();
        let cands = pattern_candidates(rule, &scan, &env);
        let opts = ClassifyOptions {
            dormancy_days: 60,
            run_probes: false,
            in_use_checks: true,
            ..Default::default()
        };
        let recs: Vec<Recommendation> = cands
            .into_iter()
            .filter_map(|c| {
                finish(
                    rule,
                    c,
                    &scan,
                    &activity,
                    &opts,
                    &AtimeContext::default(),
                    false,
                )
            })
            .collect();
        let old = recs
            .iter()
            .find(|r| r.path.as_deref().unwrap().ends_with("venvs/old"))
            .unwrap();
        assert!(old.in_use.is_some(), "flagged in use");
    }

    #[test]
    fn a_running_process_alone_does_not_flag_items_with_a_path() {
        let tmp = fake_home();
        let home = tmp.path();
        let env = Env::with_home(home);
        let scan = scan(
            home,
            ScanOptions {
                big_file_threshold: 1_000_000,
                one_filesystem: true,
                progress: None,
            },
        )
        .unwrap();
        let rules = RuleSet::embedded().unwrap();
        // `node` is "running" (its process name is known) but holds nothing open here
        let activity = Activity::for_test_with_processes(vec![], vec!["node".into()]);
        let rule = rules.get("node-modules").unwrap();
        let cands = pattern_candidates(rule, &scan, &env);
        let opts = ClassifyOptions {
            dormancy_days: 60,
            run_probes: false,
            in_use_checks: true,
            ..Default::default()
        };
        let recs: Vec<Recommendation> = cands
            .into_iter()
            .filter_map(|c| {
                finish(
                    rule,
                    c,
                    &scan,
                    &activity,
                    &opts,
                    &AtimeContext::default(),
                    false,
                )
            })
            .collect();
        assert!(!recs.is_empty());
        assert!(recs.iter().all(|r| r.in_use.is_none()), "{recs:?}");
    }

    #[test]
    fn substitutes_fields_and_quotes_paths() {
        let mut c = Candidate {
            key: "k".into(),
            display: "d".into(),
            path: Some(PathBuf::from("/x/it's")),
            size: 1,
            reclaimable: None,
            last_used: None,
            project: Some(PathBuf::from("/p")),
            group: None,
            fields: BTreeMap::new(),
            note: None,
            in_use: None,
        };
        c.fields.insert("org".into(), "".into());
        c.fields.insert("name".into(), "gpt2".into());
        assert_eq!(
            substitute("hf cache rm model/{org}/{name} --yes", &c).unwrap(),
            "hf cache rm model/gpt2 --yes"
        );
        assert_eq!(
            substitute("cargo clean --manifest-path {project}/Cargo.toml", &c).unwrap(),
            "cargo clean --manifest-path '/p'/Cargo.toml"
        );
        assert!(substitute("snap remove {missing}", &c).is_none());
    }
}
