//! Path templates used by rules: `~`, `$VAR` and `${VAR}` expansion, brace expansion for
//! globs, and small helpers shared by the scanner and the classifier.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Environment seen by the rules: real variables plus a home directory. Kept as a struct
/// so tests can build one without touching the process environment.
#[derive(Debug, Clone)]
pub struct Env {
    pub home: PathBuf,
    pub vars: BTreeMap<String, String>,
}

impl Env {
    /// The current process environment (`$HOME` or `/root` when unset).
    pub fn current() -> Env {
        let vars: BTreeMap<String, String> = std::env::vars().collect();
        let home = vars
            .get("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/root"));
        Env { home, vars }
    }

    pub fn with_home(home: impl Into<PathBuf>) -> Env {
        Env {
            home: home.into(),
            vars: BTreeMap::new(),
        }
    }

    pub fn set(mut self, key: &str, value: &str) -> Env {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }

    /// Expand a rule template. Returns `None` when the template references a variable that
    /// is not set: such a template is skipped, as `docs/rules.md` says.
    pub fn expand(&self, template: &str) -> Option<PathBuf> {
        let mut out = String::with_capacity(template.len());
        let mut chars = template.char_indices().peekable();
        let mut first = true;
        while let Some((i, c)) = chars.next() {
            match c {
                '~' if first && (template.len() == 1 || template[i + 1..].starts_with('/')) => {
                    out.push_str(&self.home.to_string_lossy());
                }
                '$' => {
                    let braced = matches!(chars.peek(), Some((_, '{')));
                    if braced {
                        chars.next();
                    }
                    let mut name = String::new();
                    while let Some((_, nc)) = chars.peek() {
                        if nc.is_ascii_alphanumeric() || *nc == '_' {
                            name.push(*nc);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if braced {
                        match chars.next() {
                            Some((_, '}')) => {}
                            _ => return None,
                        }
                    }
                    if name.is_empty() {
                        return None;
                    }
                    let value = if name == "HOME" {
                        Some(self.home.to_string_lossy().to_string())
                    } else {
                        self.vars.get(&name).cloned()
                    };
                    match value {
                        Some(v) if !v.is_empty() => out.push_str(&v),
                        _ => return None,
                    }
                }
                _ => out.push(c),
            }
            first = false;
        }
        Some(PathBuf::from(out))
    }
}

/// Expand `{a,b}` groups into every combination: `~/.config/{Code,Cursor}/Cache` gives two
/// strings. `globset` has no brace support, so rules are pre-expanded here.
pub fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let Some(close_rel) = pattern[open..].find('}') else {
        return vec![pattern.to_string()];
    };
    let close = open + close_rel;
    let head = &pattern[..open];
    let body = &pattern[open + 1..close];
    let tail = &pattern[close + 1..];
    let mut out = Vec::new();
    for alt in body.split(',') {
        for rest in expand_braces(tail) {
            out.push(format!("{head}{alt}{rest}"));
        }
    }
    out
}

/// True when `path` has `fragment` somewhere in it as a path component sequence, the way
/// `exclude_under = ["docker/volumes"]` means "anything below a `docker/volumes` folder".
pub fn contains_fragment(path: &Path, fragment: &str) -> bool {
    let frag = fragment.trim_end_matches('/');
    let frag_parts: Vec<&str> = frag.split('/').filter(|p| !p.is_empty()).collect();
    if frag_parts.is_empty() {
        return false;
    }
    let parts: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    parts
        .windows(frag_parts.len())
        .any(|w| w.iter().zip(&frag_parts).all(|(a, b)| a == b))
}

/// Shell-quote a path for the generated script (single quotes, the only safe quoting).
pub fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_home_and_variables() {
        let env = Env::with_home("/home/alex").set("UV_CACHE_DIR", "/data/uv");
        assert_eq!(
            env.expand("~/.cache/pip"),
            Some(PathBuf::from("/home/alex/.cache/pip"))
        );
        assert_eq!(env.expand("$UV_CACHE_DIR"), Some(PathBuf::from("/data/uv")));
        assert_eq!(
            env.expand("${UV_CACHE_DIR}/x"),
            Some(PathBuf::from("/data/uv/x"))
        );
        assert_eq!(
            env.expand("$HOME/.npm"),
            Some(PathBuf::from("/home/alex/.npm"))
        );
        assert_eq!(
            env.expand("$PIP_CACHE_DIR"),
            None,
            "unset variable skips the template"
        );
        assert_eq!(
            env.expand("/var/cache/apt"),
            Some(PathBuf::from("/var/cache/apt"))
        );
        assert_eq!(
            env.expand("~user/x"),
            Some(PathBuf::from("~user/x")),
            "~user is not expanded"
        );
    }

    #[test]
    fn expands_braces() {
        let mut v = expand_braces("~/.config/{Code,Cursor}/{Cache,logs}");
        v.sort();
        assert_eq!(
            v,
            vec![
                "~/.config/Code/Cache",
                "~/.config/Code/logs",
                "~/.config/Cursor/Cache",
                "~/.config/Cursor/logs"
            ]
        );
        assert_eq!(expand_braces("plain"), vec!["plain"]);
    }

    #[test]
    fn fragments_and_quoting() {
        assert!(contains_fragment(
            Path::new("/a/stack/docker/volumes/x/.venv"),
            "docker/volumes"
        ));
        assert!(!contains_fragment(
            Path::new("/a/dockerx/volumes"),
            "docker/volumes"
        ));
        assert!(contains_fragment(
            Path::new("/p/node_modules/q"),
            "node_modules/"
        ));
        assert_eq!(shell_quote("/it's here"), "'/it'\\''s here'");
    }
}
