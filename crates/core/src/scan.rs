//! Disk scanner: a parallel walk that produces a tree of directories with true sizes.
//!
//! True, as `du` means it (R5 of the brief):
//! - sizes are **allocated blocks**, not apparent length (a sparse 10 GB image that uses
//!   2 GB counts 2 GB);
//! - a file with several hard links is counted **once**, on the first link met;
//! - symbolic links are never followed (their own few bytes count, their target does not);
//! - the walk never leaves the filesystem of the root (`-x` of `du`), and never enters
//!   `/proc`, `/sys`, `/dev`, `/run`;
//! - only metadata is read (R10 bis).
//!
//! Memory: one node per directory, plus the files above `big_file_threshold` and the
//! marker files the rules care about. Individual small files are summed, not stored.

use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use rayon::prelude::*;
use serde::Serialize;

/// Files whose presence in a directory the classifier wants to know about.
pub const MARKERS: &[&str] = &[
    ".git",
    "package.json",
    "pyproject.toml",
    "setup.py",
    "requirements.txt",
    "uv.lock",
    "Cargo.toml",
    "CACHEDIR.TAG",
    "pyvenv.cfg",
    "conda-meta",
    "go.mod",
    "environment.yml",
    "Dockerfile",
    "docker-compose.yml",
    "compose.yml",
];

/// Directory names that never count as "the project was touched" when computing a
/// project's activity: dependencies and build outputs change on every install.
pub const BUILD_DIRS: &[&str] = &[
    "node_modules",
    ".venv",
    "venv",
    "target",
    ".next",
    "dist",
    "build",
    ".turbo",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".git",
    ".cache",
    ".open-next",
    ".nuxt",
    ".svelte-kit",
    ".parcel-cache",
    ".astro",
    ".vite",
];

const PSEUDO_FS: &[&str] = &["/proc", "/sys", "/dev", "/run"];

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    /// Allocated bytes (0 when this is a further hard link of a file already counted).
    pub size: u64,
    pub apparent_size: u64,
    pub mtime: i64,
    pub atime: i64,
    pub nlink: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirNode {
    pub path: PathBuf,
    pub name: String,
    /// Allocated bytes of everything below, hard links counted once.
    pub size: u64,
    /// Allocated bytes of the files directly inside (not subdirectories).
    pub own_size: u64,
    pub files: u64,
    pub dirs: u64,
    /// Newest modification time of anything below (files and directories).
    pub newest_mtime: i64,
    /// Newest modification time ignoring `BUILD_DIRS`: "when was this project touched".
    pub newest_source_mtime: i64,
    /// Newest access time of a file below.
    pub newest_atime: i64,
    /// Marker files present directly in this directory (see `MARKERS`).
    pub markers: Vec<String>,
    /// Files at or above the scan threshold, directly inside this directory.
    pub big_files: Vec<FileEntry>,
    pub children: Vec<DirNode>,
    /// Permission or I/O problem met here (the directory is kept with what was readable).
    pub error: Option<String>,
    /// True when this is a mount point of another filesystem (not entered).
    pub other_fs: bool,
}

impl DirNode {
    fn leaf(path: PathBuf) -> DirNode {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        DirNode {
            path,
            name,
            size: 0,
            own_size: 0,
            files: 0,
            dirs: 0,
            newest_mtime: 0,
            newest_source_mtime: 0,
            newest_atime: 0,
            markers: Vec::new(),
            big_files: Vec::new(),
            children: Vec::new(),
            error: None,
            other_fs: false,
        }
    }

    pub fn has_marker(&self, marker: &str) -> bool {
        self.markers.iter().any(|m| m == marker)
    }

    /// Pre-order iteration over this node and everything below.
    pub fn iter(&self) -> impl Iterator<Item = &DirNode> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let node = stack.pop()?;
            stack.extend(node.children.iter().rev());
            Some(node)
        })
    }

    /// The node for `path`, if it is this node or below it.
    pub fn find(&self, path: &Path) -> Option<&DirNode> {
        let rel = path.strip_prefix(&self.path).ok()?;
        let mut node = self;
        for comp in rel.components() {
            let name = comp.as_os_str().to_string_lossy();
            node = node.children.iter().find(|c| c.name == name)?;
        }
        Some(node)
    }

    /// Drop children deeper than `depth` (for compact JSON output).
    pub fn pruned(&self, depth: usize) -> DirNode {
        let mut copy = self.clone();
        copy.prune(depth);
        copy
    }

    fn prune(&mut self, depth: usize) {
        if depth == 0 {
            self.children.clear();
            self.big_files.clear();
        } else {
            for c in &mut self.children {
                c.prune(depth - 1);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub root: PathBuf,
    pub total_size: u64,
    pub files: u64,
    pub dirs: u64,
    pub errors: u64,
    /// Mount points of other filesystems met and not entered.
    pub other_filesystems: Vec<PathBuf>,
    /// Hard-linked files met more than once (their later links counted 0).
    pub hardlinks_deduplicated: u64,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct Scan {
    pub summary: ScanSummary,
    pub tree: DirNode,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Files at or above this size are kept individually (default 1 MiB: enough to list
    /// session transcripts and model shards without storing every small file).
    pub big_file_threshold: u64,
    /// Stay on the root's filesystem (default true, like `du -x`).
    pub one_filesystem: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            big_file_threshold: 1024 * 1024,
            one_filesystem: true,
        }
    }
}

struct Shared {
    opts: ScanOptions,
    root_dev: u64,
    seen_inodes: Mutex<HashSet<(u64, u64)>>,
    errors: AtomicU64,
    dedup: AtomicU64,
    other_fs: Mutex<Vec<PathBuf>>,
    marker_set: HashSet<&'static str>,
}

/// Scan `root` (read-only). Errors inside the tree are recorded on their node; only a
/// root that cannot be read at all is an error.
pub fn scan(root: &Path, opts: ScanOptions) -> std::io::Result<Scan> {
    let start = Instant::now();
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let meta = std::fs::symlink_metadata(&root)?;
    if !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a directory", root.display()),
        ));
    }
    let shared = Shared {
        opts,
        root_dev: meta.dev(),
        seen_inodes: Mutex::new(HashSet::new()),
        errors: AtomicU64::new(0),
        dedup: AtomicU64::new(0),
        other_fs: Mutex::new(Vec::new()),
        marker_set: MARKERS.iter().copied().collect(),
    };
    let tree = scan_dir(&shared, root.clone());
    let mut other = shared.other_fs.into_inner().unwrap_or_default();
    other.sort();
    let summary = ScanSummary {
        root,
        total_size: tree.size,
        files: tree.files,
        dirs: tree.dirs,
        errors: shared.errors.load(Ordering::Relaxed),
        other_filesystems: other,
        hardlinks_deduplicated: shared.dedup.load(Ordering::Relaxed),
        duration_ms: start.elapsed().as_millis(),
    };
    Ok(Scan { summary, tree })
}

fn scan_dir(shared: &Shared, path: PathBuf) -> DirNode {
    let mut node = DirNode::leaf(path.clone());
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        node.newest_mtime = meta.mtime();
        node.newest_source_mtime = meta.mtime();
        // Directory entries themselves use blocks too (large directories cost MBs).
        node.own_size = meta.blocks() * 512;
    }
    let entries = match std::fs::read_dir(&path) {
        Ok(rd) => rd,
        Err(e) => {
            shared.errors.fetch_add(1, Ordering::Relaxed);
            node.error = Some(e.to_string());
            node.size = node.own_size;
            return node;
        }
    };

    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                shared.errors.fetch_add(1, Ordering::Relaxed);
                node.error = Some(e.to_string());
                continue;
            }
        };
        let epath = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = match std::fs::symlink_metadata(&epath) {
            Ok(m) => m,
            Err(e) => {
                shared.errors.fetch_add(1, Ordering::Relaxed);
                node.error = Some(e.to_string());
                continue;
            }
        };
        let ft = meta.file_type();
        if shared.marker_set.contains(name.as_str()) {
            node.markers.push(name.clone());
        }
        if ft.is_dir() {
            if PSEUDO_FS.iter().any(|p| Path::new(p) == epath) {
                continue;
            }
            if shared.opts.one_filesystem && meta.dev() != shared.root_dev {
                let mut other = DirNode::leaf(epath.clone());
                other.other_fs = true;
                node.children.push(other);
                shared.other_fs.lock().unwrap().push(epath);
                continue;
            }
            subdirs.push(epath);
            continue;
        }
        // Regular files, symlinks (their own size), sockets, fifos: count blocks.
        let allocated = meta.blocks() * 512;
        let counted = if ft.is_file() && meta.nlink() > 1 {
            let key = (meta.dev(), meta.ino());
            let first = shared.seen_inodes.lock().unwrap().insert(key);
            if first {
                allocated
            } else {
                shared.dedup.fetch_add(1, Ordering::Relaxed);
                0
            }
        } else {
            allocated
        };
        node.own_size += counted;
        node.files += 1;
        node.newest_mtime = node.newest_mtime.max(meta.mtime());
        node.newest_source_mtime = node.newest_source_mtime.max(meta.mtime());
        node.newest_atime = node.newest_atime.max(meta.atime());
        if ft.is_file() && allocated.max(meta.len()) >= shared.opts.big_file_threshold {
            node.big_files.push(FileEntry {
                path: epath,
                name,
                size: counted,
                apparent_size: meta.len(),
                mtime: meta.mtime(),
                atime: meta.atime(),
                nlink: meta.nlink(),
            });
        }
    }

    let mut children: Vec<DirNode> = subdirs
        .into_par_iter()
        .map(|p| scan_dir(shared, p))
        .collect();
    children.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    node.big_files.sort_by_key(|f| std::cmp::Reverse(f.size));

    let mut size = node.own_size;
    for c in &children {
        size += c.size;
        node.files += c.files;
        node.dirs += c.dirs + 1;
        node.newest_mtime = node.newest_mtime.max(c.newest_mtime);
        node.newest_atime = node.newest_atime.max(c.newest_atime);
        if !BUILD_DIRS.contains(&c.name.as_str()) {
            node.newest_source_mtime = node.newest_source_mtime.max(c.newest_source_mtime);
        }
    }
    node.size = size;
    node.children.extend(children);
    node
}

/// Allocated size of a path outside the scanned tree (e.g. `/var/cache/apt`), with the
/// same rules; unreadable parts are skipped. Returns `None` when the path does not exist.
pub fn size_of(path: &Path) -> Option<DirNode> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.is_dir() {
        scan(path, ScanOptions::default()).ok().map(|s| s.tree)
    } else {
        let mut node = DirNode::leaf(path.to_path_buf());
        node.size = meta.blocks() * 512;
        node.own_size = node.size;
        node.files = 1;
        node.newest_mtime = meta.mtime();
        node.newest_source_mtime = meta.mtime();
        node.newest_atime = meta.atime();
        Some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(path).unwrap();
        f.write_all(&vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn counts_blocks_once_per_inode_and_records_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("proj/package.json"), 10);
        write(&root.join("proj/node_modules/a/index.js"), 300_000);
        write(&root.join("proj/src/main.js"), 1000);
        write(&root.join("venv/pyvenv.cfg"), 10);
        write(&root.join("big.bin"), 2_000_000);
        fs::hard_link(root.join("big.bin"), root.join("big-link.bin")).unwrap();
        std::os::unix::fs::symlink(root.join("big.bin"), root.join("link")).unwrap();

        let opts = ScanOptions {
            big_file_threshold: 1_000_000,
            one_filesystem: true,
        };
        let scan = scan(root, opts).unwrap();
        let tree = &scan.tree;

        // du semantics: the hard link is not counted twice.
        let big_once = 2_000_000u64.div_ceil(4096) * 4096;
        assert!(
            tree.size >= big_once && tree.size < 2 * big_once,
            "size {}",
            tree.size
        );
        assert_eq!(scan.summary.hardlinks_deduplicated, 1);
        assert_eq!(
            tree.big_files.len(),
            2,
            "both links are listed, one with size 0"
        );
        assert!(tree.big_files.iter().any(|f| f.size == 0));

        let proj = tree.find(&root.join("proj")).unwrap();
        assert!(proj.has_marker("package.json"));
        let venv = tree.find(&root.join("venv")).unwrap();
        assert!(venv.has_marker("pyvenv.cfg"));
        assert_eq!(scan.summary.files, 7, "6 files + 1 symlink");
        assert!(tree.iter().count() >= 6);
    }

    #[test]
    fn source_mtime_ignores_build_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("proj/package.json"), 10);
        write(&root.join("proj/src/a.js"), 10);
        write(&root.join("proj/node_modules/x.js"), 10);
        let old = filetime_secs(1_600_000_000);
        set_mtime(&root.join("proj/src/a.js"), old);
        set_mtime(&root.join("proj/package.json"), old);
        set_mtime(&root.join("proj/src"), old);
        set_mtime(&root.join("proj"), old);
        let scan = scan(root, ScanOptions::default()).unwrap();
        let proj = scan.tree.find(&root.join("proj")).unwrap();
        assert!(proj.newest_mtime > old, "node_modules is recent");
        assert_eq!(
            proj.newest_source_mtime, old,
            "but the project itself is old"
        );
    }

    fn filetime_secs(secs: i64) -> i64 {
        secs
    }

    fn set_mtime(path: &Path, secs: i64) {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
        let f = fs::File::options().write(true).open(path).ok();
        match f {
            Some(f) => f.set_modified(t).unwrap(),
            None => {
                // directories: use utime through the same std API on a handle
                let d = fs::File::open(path).unwrap();
                d.set_modified(t).unwrap();
            }
        }
    }
}
