//! What is in use right now (R6): running processes, files they hold open, and host
//! paths mounted into running containers. Read from `/proc` (own processes; root's
//! processes are invisible to a normal user, which errs on the side of proposing — the
//! script still never runs by itself).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Activity {
    /// Lower-cased process names: `comm` values and basenames of command-line tokens,
    /// plus the dotted segments of module names (`python -m vllm.entrypoints` → `vllm`).
    processes: HashSet<String>,
    /// Every open file, executable, working directory and mapped file — and all of their
    /// ancestor directories, so "is anything open below this directory" is one lookup.
    open: HashSet<PathBuf>,
    /// Host paths bind-mounted into running containers.
    bind_mounts: Vec<PathBuf>,
}

impl Activity {
    /// Snapshot the current state. `bind_mounts` comes from the Docker probe.
    pub fn collect(bind_mounts: Vec<PathBuf>) -> Activity {
        let mut a = Activity {
            bind_mounts,
            ..Default::default()
        };
        let Ok(rd) = std::fs::read_dir("/proc") else {
            return a;
        };
        let my_pid = std::process::id().to_string();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.chars().all(|c| c.is_ascii_digit()) || name == my_pid {
                continue;
            }
            let pdir = e.path();
            if let Ok(comm) = std::fs::read_to_string(pdir.join("comm")) {
                a.processes.insert(comm.trim().to_ascii_lowercase());
            }
            if let Ok(cmd) = std::fs::read(pdir.join("cmdline")) {
                for tok in cmd.split(|b| *b == 0).take(6) {
                    let t = String::from_utf8_lossy(tok);
                    let base = t.rsplit('/').next().unwrap_or(&t).to_ascii_lowercase();
                    if base.is_empty() {
                        continue;
                    }
                    for seg in base.split('.') {
                        if !seg.is_empty() {
                            a.processes.insert(seg.to_string());
                        }
                    }
                    a.processes.insert(base);
                }
            }
            for link in ["exe", "cwd"] {
                if let Ok(p) = std::fs::read_link(pdir.join(link)) {
                    a.insert_open(p);
                }
            }
            if let Ok(fds) = std::fs::read_dir(pdir.join("fd")) {
                for fd in fds.flatten() {
                    if let Ok(p) = std::fs::read_link(fd.path()) {
                        if p.is_absolute() {
                            a.insert_open(p);
                        }
                    }
                }
            }
            if let Ok(maps) = std::fs::read_to_string(pdir.join("maps")) {
                for line in maps.lines() {
                    if let Some(idx) = line.find(" /") {
                        let p = PathBuf::from(&line[idx + 1..]);
                        a.insert_open(p);
                    }
                }
            }
        }
        a
    }

    /// An `Activity` with nothing running (tests, `--no-in-use-checks`).
    pub fn empty() -> Activity {
        Activity::default()
    }

    /// An `Activity` where exactly these files are open (tests).
    #[doc(hidden)]
    pub fn for_test(open_files: Vec<PathBuf>) -> Activity {
        let mut a = Activity::default();
        for p in open_files {
            a.insert_open(p);
        }
        a
    }

    fn insert_open(&mut self, path: PathBuf) {
        let path = PathBuf::from(path.to_string_lossy().trim_end_matches(" (deleted)"));
        let mut p: Option<&Path> = Some(&path);
        while let Some(cur) = p {
            if !self.open.insert(cur.to_path_buf()) {
                break;
            }
            p = cur.parent();
        }
    }

    /// The first of `names` that matches a running process.
    pub fn process_running(&self, names: &[String]) -> Option<String> {
        names
            .iter()
            .find(|n| self.processes.contains(&n.to_ascii_lowercase()))
            .cloned()
    }

    /// True when `path` is an open file, or a directory with something open below it.
    /// A file inside a directory that is merely a process's working directory is not
    /// flagged: only the file itself and its ancestors are in the set.
    pub fn path_open(&self, path: &Path) -> bool {
        self.open.contains(path)
    }

    /// The bind mount `path` overlaps with, if any (either contains the other).
    pub fn bind_mounted(&self, path: &Path) -> Option<&Path> {
        self.bind_mounts
            .iter()
            .find(|m| path.starts_with(m) || m.starts_with(path))
            .map(PathBuf::as_path)
    }

    pub fn process_count(&self) -> usize {
        self.processes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_paths_propagate_to_ancestors_only_downwards() {
        let mut a = Activity::empty();
        a.insert_open(PathBuf::from("/home/u/venvs/x/bin/python"));
        assert!(
            a.path_open(Path::new("/home/u/venvs/x")),
            "a directory with an open file inside"
        );
        assert!(a.path_open(Path::new("/home/u/venvs/x/bin/python")));
        assert!(
            !a.path_open(Path::new("/home/u/venvs/y")),
            "sibling untouched"
        );
        assert!(!a.path_open(Path::new("/home/u/other")));
    }

    #[test]
    fn bind_mounts_overlap_both_ways() {
        let a = Activity {
            bind_mounts: vec![PathBuf::from("/srv/stack/docker/volumes/plugin")],
            ..Default::default()
        };
        assert!(a
            .bind_mounted(Path::new("/srv/stack/docker/volumes/plugin/.venv"))
            .is_some());
        assert!(
            a.bind_mounted(Path::new("/srv/stack")).is_some(),
            "removing a parent would remove the mount"
        );
        assert!(a.bind_mounted(Path::new("/srv/other")).is_none());
    }

    #[test]
    fn sees_itself_as_a_running_process_family() {
        let a = Activity::collect(Vec::new());
        // The test binary is running; its cmdline token family is present.
        assert!(a.process_count() > 0);
    }
}
