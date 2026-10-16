//! Rule files (`rules/*.toml`): data model, loading, validation. The schema is documented
//! in `docs/rules.md`; this module is its executable counterpart and refuses anything the
//! document does not allow.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};

use crate::size::parse_size;

/// Safety level of a rule (`docs/rules.md`, "Levels").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Regenerable without loss; removed outright by the script.
    Safe,
    /// Recoverable but costly or ambiguous; moved to the trash, ticked explicitly.
    Review,
    /// Shown in the treemap only, never in the script.
    Never,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Safe => "safe",
            Level::Review => "review",
            Level::Never => "never",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectKind {
    Path,
    Signature,
    Pattern,
    Probe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanupKind {
    Command,
    Rm,
    Trash,
    None,
}

/// A string or a list of strings (`glob = "x"` or `glob = ["x", "y"]`).
fn one_or_many<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(s) => vec![s],
        OneOrMany::Many(v) => v,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detect {
    pub kind: DetectKind,
    /// `path`: templates tried in order; `probe`: hints for the probe.
    #[serde(default)]
    pub paths: Vec<String>,
    /// `path`: one item per child matching this glob instead of the directory itself.
    #[serde(default)]
    pub item_glob: Option<String>,
    /// `pattern`: directory globs (`**/node_modules`, `~/.config/{Code,Cursor}/Cache`).
    #[serde(default, deserialize_with = "one_or_many")]
    pub glob: Vec<String>,
    /// `pattern`: a file whose presence identifies the directory (`pyvenv.cfg`).
    #[serde(default)]
    pub marker: Option<String>,
    /// `pattern`: `"project"` attributes the match to the nearest project ancestor.
    #[serde(default)]
    pub anchor: Option<String>,
    /// `pattern`: the parent must hold one of these files (`dist/` next to `package.json`).
    #[serde(default)]
    pub requires_sibling: Vec<String>,
    /// Any kind: skip matches below a path containing one of these fragments.
    #[serde(default)]
    pub exclude_under: Vec<String>,
    /// `signature`: file extensions, multi-dot allowed (`tar.gz`); `*` means any file.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// `signature` / `pattern` / `probe`: ignore items smaller than this.
    #[serde(default)]
    pub min_size: Option<String>,
    /// `signature`: files inside a managed store belong to that store's rule.
    #[serde(default)]
    pub exclude_managed: bool,
    /// `probe`: probe name (`docker`, `ollama`, `hf-cache`, …).
    #[serde(default)]
    pub probe: Option<String>,
    /// `probe`: which items of the probe (`build-cache`, `unused-volumes`, …).
    #[serde(default)]
    pub item: Option<String>,
    /// How the UI groups items (`project`, `app`, `parent:N`).
    #[serde(default)]
    pub group_by: Option<String>,
    /// `probe = "git-status"`: where to look.
    #[serde(default)]
    pub roots: Vec<String>,
    /// `pattern`: skip matches that sit inside a project (an ancestor with `.git`,
    /// `package.json`, `pyproject.toml`, `Cargo.toml`): a backup folder inside a repository
    /// is the repository's business.
    #[serde(default)]
    pub skip_in_project: bool,
}

impl Detect {
    pub fn min_size_bytes(&self) -> Option<u64> {
        self.min_size.as_deref().and_then(parse_size)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cleanup {
    pub kind: CleanupKind,
    #[serde(default)]
    pub command: Option<String>,
    /// `"rm"`, `"trash"` or `"none"`: what to do when the tool is not installed.
    #[serde(default)]
    pub fallback: Option<String>,
    /// `user`, `sudo` or `docker-group-or-sudo`.
    #[serde(default)]
    pub requires: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explain {
    pub why: String,
    pub regenerate: String,
    pub cost: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conditions {
    #[serde(default)]
    pub dormancy_days: Option<u32>,
    /// `mtime` (default) or `atime:<relative file>`.
    #[serde(default)]
    pub dormancy_measure: Option<String>,
    /// Pattern rules: level used when the parent project is active.
    #[serde(default)]
    pub active_project_level: Option<Level>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InUse {
    #[serde(default)]
    pub processes: Vec<String>,
    #[serde(default)]
    pub probe: Option<String>,
    #[serde(default)]
    pub open_files: Option<bool>,
    #[serde(default)]
    pub bind_mounts: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    pub read_on: toml::value::Datetime,
    #[serde(default)]
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub category: String,
    pub level: Level,
    pub detect: Detect,
    pub cleanup: Cleanup,
    pub explain: Explain,
    #[serde(default)]
    pub conditions: Option<Conditions>,
    #[serde(default)]
    pub in_use: Option<InUse>,
    pub source: Source,
}

impl Rule {
    /// Dormancy window in days, with the documented default of 60 for rules that set a
    /// condition block without a value.
    pub fn dormancy_days(&self, default_days: u32) -> Option<u32> {
        self.conditions
            .as_ref()
            .map(|c| c.dormancy_days.unwrap_or(default_days))
    }

    /// Whether the rule's source still needs to be read before a release. `observed`
    /// sources (layout seen on a real machine, no public documentation) are complete as
    /// long as they say what was seen.
    pub fn source_to_verify(&self) -> bool {
        self.source.url == "to-verify"
            || self.source.quote.starts_with('(')
            || (self.source.url == "observed" && self.source.quote.trim().is_empty())
    }

    /// The source documents the layout only by observation, not by documentation.
    pub fn source_is_observation(&self) -> bool {
        self.source.url == "observed"
    }

    /// `open_files` check: explicit value, else true for `review` rules and for any rule
    /// that names processes (the precise check replaces the coarse "is X running" one
    /// whenever the item has a path).
    pub fn checks_open_files(&self) -> bool {
        self.in_use.as_ref().and_then(|u| u.open_files).unwrap_or(
            self.level == Level::Review
                || self
                    .in_use
                    .as_ref()
                    .is_some_and(|u| !u.processes.is_empty()),
        )
    }

    /// The catch-all signature rule (`extensions = ["*"]`) runs after every other rule, so
    /// it only lists what nothing else classified.
    pub fn is_catch_all(&self) -> bool {
        self.detect.kind == DetectKind::Signature && self.detect.extensions.iter().any(|e| e == "*")
    }

    /// `bind_mounts` check: explicit value, else true for `review` rules.
    pub fn checks_bind_mounts(&self) -> bool {
        self.in_use
            .as_ref()
            .and_then(|u| u.bind_mounts)
            .unwrap_or(self.level == Level::Review)
    }
}

#[derive(Debug, Deserialize)]
struct RuleFile {
    schema: u32,
    #[serde(default)]
    rule: Vec<Rule>,
}

#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    #[error("{file}: cannot read: {source}")]
    Read {
        file: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{file}: invalid TOML: {source}")]
    Parse {
        file: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("{file}: unsupported schema {schema} (expected 1)")]
    Schema { file: String, schema: u32 },
    #[error("rule `{id}`: {problem}")]
    Invalid { id: String, problem: String },
}

/// The rule files shipped inside the binary, so `cleanator-cli` works without a `rules/`
/// directory next to it. Kept in sync with the repository by `include_str!`.
pub const EMBEDDED: &[(&str, &str)] = &[
    (
        "assistants.toml",
        include_str!("../../../rules/assistants.toml"),
    ),
    (
        "backups-and-large-files.toml",
        include_str!("../../../rules/backups-and-large-files.toml"),
    ),
    ("builds.toml", include_str!("../../../rules/builds.toml")),
    ("docker.toml", include_str!("../../../rules/docker.toml")),
    ("models.toml", include_str!("../../../rules/models.toml")),
    (
        "package-caches.toml",
        include_str!("../../../rules/package-caches.toml"),
    ),
    (
        "python-envs.toml",
        include_str!("../../../rules/python-envs.toml"),
    ),
    ("system.toml", include_str!("../../../rules/system.toml")),
    ("tooling.toml", include_str!("../../../rules/tooling.toml")),
    (
        "training.toml",
        include_str!("../../../rules/training.toml"),
    ),
];

/// A validated set of rules.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
    /// Where they came from (`embedded` or a directory), for reports.
    pub origin: String,
}

impl RuleSet {
    /// Parse the rules embedded in the binary.
    pub fn embedded() -> Result<RuleSet, RuleError> {
        let mut rules = Vec::new();
        for (name, text) in EMBEDDED {
            rules.extend(parse_file(name, text)?);
        }
        let set = RuleSet {
            rules,
            origin: "embedded".to_string(),
        };
        set.validate()?;
        Ok(set)
    }

    /// Parse every `*.toml` file of a directory (sorted by name).
    pub fn from_dir(dir: &Path) -> Result<RuleSet, RuleError> {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| RuleError::Read {
                file: dir.display().to_string(),
                source: e,
            })?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        files.sort();
        let mut rules = Vec::new();
        for file in files {
            let name = file.display().to_string();
            let text = std::fs::read_to_string(&file).map_err(|e| RuleError::Read {
                file: name.clone(),
                source: e,
            })?;
            rules.extend(parse_file(&name, &text)?);
        }
        let set = RuleSet {
            rules,
            origin: dir.display().to_string(),
        };
        set.validate()?;
        Ok(set)
    }

    pub fn get(&self, id: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The consistency checks of `docs/rules.md`, as errors rather than conventions.
    pub fn validate(&self) -> Result<(), RuleError> {
        let mut seen = BTreeSet::new();
        for r in &self.rules {
            let invalid = |problem: String| RuleError::Invalid {
                id: r.id.clone(),
                problem,
            };
            if !seen.insert(r.id.clone()) {
                return Err(invalid("duplicate id".into()));
            }
            if r.id.is_empty()
                || !r
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                return Err(invalid("id must be kebab-case".into()));
            }
            match (r.level, r.cleanup.kind) {
                (Level::Safe, CleanupKind::None) => {
                    return Err(invalid("a `safe` rule must have a cleanup".into()))
                }
                (Level::Never, k) if k != CleanupKind::None => {
                    return Err(invalid("a `never` rule cannot have a cleanup".into()))
                }
                (Level::Review, CleanupKind::Rm) if r.id != "python-venv" => {
                    return Err(invalid(
                        "a `review` rule is trashed or cleaned by its tool, never `rm`".into(),
                    ))
                }
                _ => {}
            }
            if r.cleanup.kind == CleanupKind::Command && r.cleanup.command.is_none() {
                return Err(invalid("cleanup kind `command` needs a `command`".into()));
            }
            let d = &r.detect;
            match d.kind {
                DetectKind::Path if d.paths.is_empty() => {
                    return Err(invalid("detect kind `path` needs `paths`".into()))
                }
                DetectKind::Pattern if d.glob.is_empty() && d.marker.is_none() => {
                    return Err(invalid(
                        "detect kind `pattern` needs `glob` or `marker`".into(),
                    ))
                }
                DetectKind::Signature if d.extensions.is_empty() => {
                    return Err(invalid("detect kind `signature` needs `extensions`".into()))
                }
                DetectKind::Probe if d.probe.is_none() => {
                    return Err(invalid("detect kind `probe` needs `probe`".into()))
                }
                _ => {}
            }
            if let Some(ms) = &d.min_size {
                if parse_size(ms).is_none() {
                    return Err(invalid(format!("min_size `{ms}` is not a size")));
                }
            }
            if r.explain.why.trim().is_empty()
                || r.explain.regenerate.trim().is_empty()
                || r.explain.cost.trim().is_empty()
            {
                return Err(invalid("explain needs why, regenerate and cost".into()));
            }
            if r.source.url.trim().is_empty() {
                return Err(invalid(
                    "source needs a url (or `to-verify`, or `observed` with a quote)".into(),
                ));
            }
            if r.source.url == "observed" && r.source.quote.trim().is_empty() {
                return Err(invalid(
                    "an `observed` source must say what was seen, where and when".into(),
                ));
            }
        }
        Ok(())
    }
}

fn parse_file(name: &str, text: &str) -> Result<Vec<Rule>, RuleError> {
    let file: RuleFile = toml::from_str(text).map_err(|e| RuleError::Parse {
        file: name.to_string(),
        source: e,
    })?;
    if file.schema != 1 {
        return Err(RuleError::Schema {
            file: name.to_string(),
            schema: file.schema,
        });
    }
    Ok(file.rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_rules_load_and_validate() {
        let set = RuleSet::embedded().expect("embedded rules are valid");
        assert!(
            set.len() >= 70,
            "expected the v0 rule set, got {}",
            set.len()
        );
        assert!(set.get("pip-cache").is_some());
        assert!(set.get("docker-unused-images").unwrap().level == Level::Review);
        let to_verify: Vec<_> = set
            .rules
            .iter()
            .filter(|r| r.source_to_verify())
            .map(|r| r.id.as_str())
            .collect();
        assert!(
            to_verify.is_empty(),
            "every rule cites a read source since 2026-10-16; still to verify: {to_verify:?}"
        );
    }

    #[test]
    fn rejects_inconsistent_rules() {
        let text = r#"
schema = 1
[[rule]]
id = "bad"
name = "bad"
category = "system"
level = "never"
[rule.detect]
kind = "path"
paths = ["/x"]
[rule.cleanup]
kind = "rm"
[rule.explain]
why = "w"
regenerate = "r"
cost = "c"
[rule.source]
url = "u"
read_on = 2026-10-15
"#;
        let rules = parse_file("t", text).unwrap();
        let set = RuleSet {
            rules,
            origin: "test".into(),
        };
        let err = set.validate().unwrap_err().to_string();
        assert!(err.contains("`never` rule cannot have a cleanup"), "{err}");
    }

    #[test]
    fn review_rules_check_open_files_by_default() {
        let set = RuleSet::embedded().unwrap();
        let hf = set.get("hf-hub-models").unwrap();
        assert!(hf.checks_open_files());
        let pip = set.get("pip-cache").unwrap();
        assert!(!pip.checks_open_files());
        assert_eq!(set.get("python-venv").unwrap().dormancy_days(60), Some(60));
        assert_eq!(pip.dormancy_days(60), None);
    }
}
