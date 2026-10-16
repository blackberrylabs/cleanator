//! The generated cleanup script (R11–R13): the only thing that ever removes anything,
//! and only when the user reads it and runs it.
//!
//! - one block per item, its rule id and *why* as comments above the command;
//! - `set -euo pipefail`, every path single-quoted, no wildcard built at run time;
//! - user commands first, then the `sudo` block, which can be skipped;
//! - free space measured before and after so the user sees what was gained;
//! - `safe` items are removed, `review` items go to the trash (`gio trash`).

use std::fmt::Write as _;

use crate::classify::{Recommendation, Report};
use crate::rules::Level;
use crate::size::human;

/// Which recommendations go into the script.
#[derive(Debug, Clone)]
pub enum Selection {
    /// Every `safe` item not in use (the default "Select safe" of the UI).
    Safe,
    /// Every `safe` and `review` item not in use.
    All,
    /// Explicit recommendation ids (`rule-id::key`).
    Ids(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct ScriptOptions {
    pub selection: Selection,
    /// Include items flagged in use (never by default: the user must insist).
    pub include_in_use: bool,
}

impl Default for ScriptOptions {
    fn default() -> Self {
        ScriptOptions {
            selection: Selection::Safe,
            include_in_use: false,
        }
    }
}

fn selected<'a>(report: &'a Report, opts: &ScriptOptions) -> Vec<&'a Recommendation> {
    report
        .recommendations
        .iter()
        .filter(|r| r.command.is_some() && r.effective_level != Level::Never && !r.recent)
        .filter(|r| opts.include_in_use || r.in_use.is_none())
        .filter(|r| match &opts.selection {
            Selection::Safe => r.effective_level == Level::Safe,
            Selection::All => true,
            Selection::Ids(ids) => ids.iter().any(|i| i == &r.id),
        })
        .collect()
}

/// Render the script. Returns the text and the number of items it contains.
pub fn generate(report: &Report, opts: &ScriptOptions) -> (String, usize) {
    let items = selected(report, opts);
    let user: Vec<&&Recommendation> = items.iter().filter(|r| r.requires != "sudo").collect();
    let sudo: Vec<&&Recommendation> = items.iter().filter(|r| r.requires == "sudo").collect();
    let total: u64 = items.iter().map(|r| r.reclaimable).sum();
    let uses_gio = items
        .iter()
        .any(|r| r.command.as_deref().is_some_and(|c| c.starts_with("gio ")));

    let mut s = String::new();
    let _ = writeln!(s, "#!/usr/bin/env bash");
    let _ = writeln!(
        s,
        "# Cleanator cleanup script — generated {} by cleanator {}",
        report.generated_at, report.cleanator_version
    );
    let _ = writeln!(s, "# Scanned: {}", report.root.display());
    let _ = writeln!(s, "# Items: {} ({} expected to be freed). Read every line: nothing runs until you run this file.", items.len(), human(total));
    let _ = writeln!(s, "# `safe` items are removed outright; `review` items are moved to the trash and can be restored.");
    let _ = writeln!(s, "# Each block names its rule: https://github.com/blackberrylabs/cleanator/blob/main/docs/rules.md");
    let _ = writeln!(s, "set -euo pipefail");
    let _ = writeln!(s);
    if uses_gio {
        let _ = writeln!(s, "command -v gio >/dev/null 2>&1 || {{ echo 'gio not found (install libglib2.0-bin): trash commands cannot run' >&2; exit 1; }}");
    }
    let _ = writeln!(
        s,
        "free_before=$(df --output=avail -B1 -- {} | tail -n 1)",
        crate::paths::shell_quote(&report.root.to_string_lossy())
    );
    let _ = writeln!(s, "echo \"Free before: $((free_before / 1048576)) MiB\"");
    let _ = writeln!(s);

    let _ = writeln!(
        s,
        "# ===== 1/2 Your own files and caches ({} items) =====",
        user.len()
    );
    for r in &user {
        write_item(&mut s, r);
    }
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "# ===== 2/2 System items, need sudo ({} items) =====",
        sudo.len()
    );
    if sudo.is_empty() {
        let _ = writeln!(s, "# (none)");
    } else {
        let _ = writeln!(
            s,
            "# Skip this block by stopping here if you prefer not to use sudo."
        );
    }
    for r in &sudo {
        write_item(&mut s, r);
    }
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "free_after=$(df --output=avail -B1 -- {} | tail -n 1)",
        crate::paths::shell_quote(&report.root.to_string_lossy())
    );
    let _ = writeln!(s, "echo \"Free after: $((free_after / 1048576)) MiB — freed $(((free_after - free_before) / 1048576)) MiB\"");
    (s, items.len())
}

fn write_item(s: &mut String, r: &Recommendation) {
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "# [{}] {} — {} ({})",
        r.rule_id,
        r.display,
        human(r.reclaimable),
        r.effective_level
    );
    let _ = writeln!(s, "# why: {}", one_line(&r.why));
    let _ = writeln!(
        s,
        "# regenerate: {} ({})",
        one_line(&r.regenerate),
        one_line(&r.cost)
    );
    if let Some(n) = &r.notes {
        let _ = writeln!(s, "# note: {}", one_line(n));
    }
    if let Some(c) = &r.command {
        let _ = writeln!(s, "{c}");
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::CleanupKind;

    fn rec(
        id: &str,
        level: Level,
        cmd: &str,
        requires: &str,
        in_use: Option<&str>,
    ) -> Recommendation {
        Recommendation {
            id: format!("{id}::x"),
            rule_id: id.into(),
            rule_name: id.into(),
            category: "test".into(),
            level,
            effective_level: level,
            display: format!("/home/u/{id}"),
            path: Some(format!("/home/u/{id}")),
            size: 1024 * 1024,
            reclaimable: 1024 * 1024,
            last_used: None,
            project: None,
            group: None,
            why: "because".into(),
            regenerate: "rebuilt".into(),
            cost: "none".into(),
            notes: None,
            command: Some(cmd.into()),
            cleanup_kind: CleanupKind::Rm,
            requires: requires.into(),
            in_use: in_use.map(str::to_string),
            recent: false,
            atime_caveat: None,
            source_url: "u".into(),
            source_to_verify: false,
        }
    }

    fn report(recs: Vec<Recommendation>) -> Report {
        Report {
            cleanator_version: "0.1.0".into(),
            generated_at: "2026-10-16T00:00:00Z".into(),
            root: "/home/u".into(),
            scan: crate::scan::ScanSummary {
                root: "/home/u".into(),
                total_size: 0,
                files: 0,
                dirs: 0,
                errors: 0,
                other_filesystems: vec![],
                hardlinks_deduplicated: 0,
                duration_ms: 0,
            },
            rules_origin: "test".into(),
            rule_count: 0,
            dormancy_days: 60,
            totals: Default::default(),
            recommendations: recs,
            probes: vec![],
            rule_notes: vec![],
            atime: Default::default(),
        }
    }

    #[test]
    fn safe_selection_excludes_review_in_use_and_never() {
        let rep = report(vec![
            rec(
                "pip-cache",
                Level::Safe,
                "rm -rf -- '/home/u/pip-cache'",
                "user",
                None,
            ),
            rec(
                "apt-archives",
                Level::Safe,
                "sudo apt-get clean",
                "sudo",
                None,
            ),
            rec(
                "hf-hub-models",
                Level::Review,
                "gio trash -- '/home/u/m'",
                "user",
                None,
            ),
            rec(
                "python-venv",
                Level::Review,
                "gio trash -- '/home/u/v'",
                "user",
                Some("open"),
            ),
            rec("cuda", Level::Never, "", "user", None),
        ]);
        let (text, n) = generate(&rep, &ScriptOptions::default());
        assert_eq!(n, 2);
        assert!(text.starts_with("#!/usr/bin/env bash"));
        assert!(text.contains("set -euo pipefail"));
        assert!(text.contains("rm -rf -- '/home/u/pip-cache'"));
        assert!(text.contains("sudo apt-get clean"));
        assert!(
            !text.contains("gio trash"),
            "review items are not in the safe selection"
        );
        assert!(!text.contains("cuda"));
        let (text, n) = generate(
            &rep,
            &ScriptOptions {
                selection: Selection::All,
                include_in_use: false,
            },
        );
        assert_eq!(n, 3, "in-use item still excluded");
        assert!(text.contains("command -v gio"));
        assert!(!text.contains("/home/u/v'"));
        let (_, n) = generate(
            &rep,
            &ScriptOptions {
                selection: Selection::Ids(vec!["hf-hub-models::x".into()]),
                include_in_use: false,
            },
        );
        assert_eq!(n, 1);
    }
}
