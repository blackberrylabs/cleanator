//! Is the access time (atime) evidence of use? Two cases make it lie.
//!
//! - A filesystem mounted `noatime` never records reads: the atime stays at the copy
//!   date, and a model used yesterday looks dormant for months (the dangerous direction).
//! - A mass read — an indexer, a backup, an antivirus — reads most of the disk on one day
//!   and every atime lands on that day: everything looks recent (the safe direction, but
//!   the dates shown are wrong).
//!
//! The Linux default `relatime` is reliable for a dormancy window of weeks: the atime is
//! refreshed as soon as it is more than 24 hours old.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::scan::{DirNode, Scan, BUILD_DIRS};

/// One line of `/proc/mounts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountInfo {
    pub mount_point: PathBuf,
    pub fstype: String,
    pub options: Vec<String>,
}

impl MountInfo {
    /// `false` when the filesystem never updates access times.
    pub fn atime_reliable(&self) -> bool {
        !self.options.iter().any(|o| o == "noatime")
    }
}

/// Parse the text of `/proc/mounts` (or `/proc/self/mounts`). Octal escapes in paths
/// (`\040` for a space) are decoded.
pub fn parse_mounts(text: &str) -> Vec<MountInfo> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let _device = f.next()?;
            let mount_point = unescape(f.next()?);
            let fstype = f.next()?.to_string();
            let options = f
                .next()
                .unwrap_or("")
                .split(',')
                .filter(|o| !o.is_empty())
                .map(str::to_string)
                .collect();
            Some(MountInfo {
                mount_point: PathBuf::from(mount_point),
                fstype,
                options,
            })
        })
        .collect()
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 4 <= b.len() {
            if let Ok(code) = u8::from_str_radix(&s[i + 1..i + 4], 8) {
                out.push(code as char);
                i += 4;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

/// The mount a path lives on: the longest mount point that is a prefix of the path.
pub fn mount_for<'a>(mounts: &'a [MountInfo], path: &Path) -> Option<&'a MountInfo> {
    mounts
        .iter()
        .filter(|m| path.starts_with(&m.mount_point))
        .max_by_key(|m| m.mount_point.as_os_str().len())
}

/// The system's mounts, empty when `/proc/mounts` cannot be read.
pub fn system_mounts() -> Vec<MountInfo> {
    std::fs::read_to_string("/proc/self/mounts")
        .or_else(|_| std::fs::read_to_string("/proc/mounts"))
        .map(|t| parse_mounts(&t))
        .unwrap_or_default()
}

/// A day on which most big files were read at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MassRead {
    /// Start of that day (UTC), as a Unix timestamp.
    pub day: i64,
    /// Big files whose atime falls on that day.
    pub files: usize,
    /// Big files considered in total.
    pub total: usize,
}

impl MassRead {
    pub fn share(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.files as f64 / self.total as f64
        }
    }

    /// Whether a timestamp falls on the mass-read day.
    pub fn covers(&self, ts: i64) -> bool {
        day_of(ts) == self.day
    }
}

/// Start of the UTC day a timestamp belongs to.
pub fn day_of(ts: i64) -> i64 {
    ts.div_euclid(86_400) * 86_400
}

/// A mass read is a day that *dominates*: at least this share of the big files, at least
/// this many times the second-busiest day, and at least this many files — on a day other
/// than today or yesterday. (Measured on a real 16 950-file home: a backup day held 35 %,
/// 2.5× the next day; ordinary days hold 5–15 %.)
pub const MASS_READ_MIN_SHARE: f64 = 0.25;
pub const MASS_READ_DOMINANCE: f64 = 2.0;
pub const MASS_READ_MIN_FILES: usize = 200;

/// Look for a mass-read day among the scan's big files, excluding build directories
/// (rebuilt all the time, their atimes say nothing about the rest).
pub fn detect_mass_read(scan: &Scan, now: i64) -> Option<MassRead> {
    let mut by_day: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    let mut total = 0usize;
    collect_atimes(&scan.tree, &mut by_day, &mut total);
    let today = day_of(now);
    let yesterday = today - 86_400;
    let mut days: Vec<(i64, usize)> = by_day
        .into_iter()
        .filter(|(d, _)| *d != today && *d != yesterday)
        .collect();
    days.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let (day, files) = *days.first()?;
    let second = days.get(1).map(|(_, n)| *n).unwrap_or(0);
    if files >= MASS_READ_MIN_FILES
        && files as f64 >= total as f64 * MASS_READ_MIN_SHARE
        && files as f64 >= second as f64 * MASS_READ_DOMINANCE
    {
        Some(MassRead { day, files, total })
    } else {
        None
    }
}

/// The same decision on a histogram (for tests and for `--explain`).
pub fn mass_read_from_counts(counts: &[(i64, usize)], total: usize) -> Option<MassRead> {
    let mut days: Vec<(i64, usize)> = counts.to_vec();
    days.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let (day, files) = *days.first()?;
    let second = days.get(1).map(|(_, n)| *n).unwrap_or(0);
    (files >= MASS_READ_MIN_FILES
        && files as f64 >= total as f64 * MASS_READ_MIN_SHARE
        && files as f64 >= second as f64 * MASS_READ_DOMINANCE)
        .then_some(MassRead { day, files, total })
}

fn collect_atimes(
    node: &DirNode,
    by_day: &mut std::collections::BTreeMap<i64, usize>,
    total: &mut usize,
) {
    if BUILD_DIRS.contains(&node.name.as_str()) {
        return;
    }
    for f in &node.big_files {
        *by_day.entry(day_of(f.atime)).or_default() += 1;
        *total += 1;
    }
    for c in &node.children {
        collect_atimes(c, by_day, total);
    }
}

/// What `classify` knows about access times: the mounts (to spot `noatime`) and the
/// mass-read day, if any. Built from the system unless a test injects it.
#[derive(Debug, Clone, Default)]
pub struct AtimeContext {
    pub mounts: Vec<MountInfo>,
    pub mass_read: Option<MassRead>,
}

impl AtimeContext {
    pub fn detect(scan: &Scan, now: i64) -> AtimeContext {
        AtimeContext {
            mounts: system_mounts(),
            mass_read: detect_mass_read(scan, now),
        }
    }

    /// `Some(mount point)` when the path's filesystem is mounted `noatime`.
    pub fn noatime_mount(&self, path: &Path) -> Option<&Path> {
        mount_for(&self.mounts, path)
            .filter(|m| !m.atime_reliable())
            .map(|m| m.mount_point.as_path())
    }
}

/// Summary of the access-time assessment, for the report header.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AtimeReport {
    /// Mount points mounted `noatime` that hold at least one atime-measured item.
    pub noatime_mounts: Vec<String>,
    pub mass_read: Option<MassRead>,
    /// Items whose last use is unknown (`noatime`): classified, never proposed.
    pub unknown_items: usize,
    pub unknown_size: u64,
    /// Items last read on the mass-read day: dormant only once that day is older than the
    /// window; `deferred_until` is the day they become proposable unless used.
    pub deferred_items: usize,
    pub deferred_size: u64,
    pub deferred_until: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTS: &str = "\
sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
/dev/nvme0n1p2 / ext4 rw,relatime,discard 0 0
/dev/sda1 /mnt/data ext4 rw,noatime,nodiratime 0 0
/dev/sdb1 /mnt/my\\040disk xfs rw,noatime 0 0
tmpfs /run tmpfs rw,nosuid,nodev,noexec,relatime,size=3270096k 0 0
";

    #[test]
    fn parses_mounts_and_finds_the_longest_prefix() {
        let m = parse_mounts(MOUNTS);
        assert_eq!(m.len(), 5);
        assert_eq!(m[3].mount_point, PathBuf::from("/mnt/my disk"));
        let root = mount_for(&m, Path::new("/home/x/.cache")).unwrap();
        assert_eq!(root.mount_point, PathBuf::from("/"));
        assert!(root.atime_reliable());
        let data = mount_for(&m, Path::new("/mnt/data/models/llama.gguf")).unwrap();
        assert_eq!(data.mount_point, PathBuf::from("/mnt/data"));
        assert!(!data.atime_reliable());
        assert!(mount_for(&m, Path::new("relative")).is_none());
    }

    #[test]
    fn context_reports_noatime_mounts_only() {
        let ctx = AtimeContext {
            mounts: parse_mounts(MOUNTS),
            mass_read: None,
        };
        assert_eq!(
            ctx.noatime_mount(Path::new("/mnt/data/x")),
            Some(Path::new("/mnt/data"))
        );
        assert_eq!(ctx.noatime_mount(Path::new("/home/x")), None);
    }

    #[test]
    fn mass_read_is_a_dominating_day() {
        // Measured on a real home on 2026-10-16: 16 950 large files.
        let d = |n: i64| day_of(1_790_000_000 + n * 86_400);
        let real = [
            (d(0), 5965usize), // 2026-10-02: a backup or an indexer
            (d(7), 2363),
            (d(-30), 2231), // 2026-09-02: the earlier suspicion, only 13 %
            (d(-3), 1390),
            (d(-2), 1077),
            (d(-7), 949),
            (d(13), 587),
        ];
        let m = mass_read_from_counts(&real, 16_950).expect("the 35 % day dominates");
        assert_eq!(m.day, d(0));
        assert_eq!(m.files, 5965);
        // Two busy days of similar weight: nobody dominates, no mass read.
        assert!(mass_read_from_counts(&[(d(0), 5000), (d(1), 4000)], 16_950).is_none());
        // A dominating day that is still a small share of the disk: no mass read.
        assert!(mass_read_from_counts(&[(d(0), 1000), (d(1), 300)], 16_950).is_none());
        // Too few files to mean anything.
        assert!(mass_read_from_counts(&[(d(0), 150), (d(1), 10)], 200).is_none());
    }

    #[test]
    fn mass_read_needs_share_and_count() {
        let m = MassRead {
            day: day_of(1_756_771_200), // 2026-09-02
            files: 300,
            total: 500,
        };
        assert!(m.covers(1_756_771_200 + 3600));
        assert!(!m.covers(1_756_771_200 + 90_000));
        assert!((m.share() - 0.6).abs() < 1e-9);
    }
}
