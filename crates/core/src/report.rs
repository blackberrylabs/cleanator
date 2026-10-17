//! The `bbl-report/1` view of a classified scan, for the Blackberrylabs control tower.
//!
//! Nothing here is new: the figures are the app's cards, the findings are the rule groups
//! of the recommendations panel, the paths are the ones the panel shows. Cleanator proposes
//! and never deletes; the tower shows and never runs. The mapping onto the four common
//! severities is in [`severity_of`] and in the CHANGELOG.

use std::collections::BTreeMap;
use std::path::Path;

use bbl_report::{Action, ActionKind, Evidence, Finding, Module, Open, Report, Severity, Subject, SubjectKind, Summary, Tone};

use crate::classify::{Recommendation, Report as Classified};
use crate::rules::Level;
use crate::size::human;

/// Binary name of the app, as packaged.
pub const APP: &str = "cleanator";
/// Binary name of the command line, as packaged.
pub const CLI: &str = "cleanator-cli";

/// Above this, a recoverable volume is worth a look rather than good to know.
pub const LARGE: u64 = 1024 * 1024 * 1024;

/// Who we are, for the report header.
pub fn module() -> Module {
    Module {
        id: "cleanator".into(),
        name: "Cleanator".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        verb: "clean".into(),
        tagline: "Disk cleaner for developers who run AI on Linux — never deletes anything".into(),
        app: Some(APP.into()),
        cli: Some(CLI.into()),
        home: "https://github.com/blackberrylabs/cleanator".into(),
    }
}

/// A rule group on the common scale: by what it frees and how safely.
pub fn severity_of(level: Level, reclaimable: u64) -> Severity {
    match level {
        // Shown in the treemap, never proposed.
        Level::Never => Severity::Info,
        // A large recoverable volume is worth a look; the rest is good to know.
        Level::Safe | Level::Review if reclaimable >= LARGE => Severity::Review,
        Level::Safe | Level::Review => Severity::GoodToKnow,
    }
}

/// One group: the proposable items of one rule at one effective level.
struct Group<'a> {
    rule_id: &'a str,
    rule_name: &'a str,
    category: &'a str,
    level: Level,
    items: Vec<&'a Recommendation>,
    reclaimable: u64,
}

/// Build the report of a classified scan. `duration_ms` covers the scan and the rules.
pub fn build(classified: &Classified, duration_ms: u64) -> Report {
    let mut report = Report::new(module());
    report.duration_ms = duration_ms;

    let t = &classified.totals;
    // The cards' colours: safe is green, review is orange, as in the app.
    report.summary = vec![
        Summary { key: "scanned".into(), label: "Scanned".into(), value: human(classified.scan.total_size), tone: Tone::Neutral },
        Summary { key: "safe".into(), label: "Safe to free".into(), value: human(t.safe), tone: if t.safe == 0 { Tone::Neutral } else { Tone::Good } },
        Summary { key: "review".into(), label: "To review".into(), value: human(t.review), tone: if t.review == 0 { Tone::Neutral } else { Tone::Amber } },
        Summary { key: "in_use".into(), label: "In use".into(), value: human(t.in_use), tone: Tone::Neutral },
    ];

    // Group the proposable recommendations by rule and effective level, as the panel does.
    let mut groups: BTreeMap<(String, Level), Group> = BTreeMap::new();
    for r in &classified.recommendations {
        if r.recent || r.negligible || r.in_use.is_some() {
            continue;
        }
        let g = groups.entry((r.rule_id.clone(), r.effective_level)).or_insert_with(|| Group {
            rule_id: &r.rule_id,
            rule_name: &r.rule_name,
            category: &r.category,
            level: r.effective_level,
            items: Vec::new(),
            reclaimable: 0,
        });
        g.items.push(r);
        g.reclaimable += if r.effective_level == Level::Never { r.size } else { r.reclaimable };
    }
    let mut groups: Vec<Group> = groups.into_values().collect();
    groups.sort_by(|a, b| a.level.cmp(&b.level).then_with(|| b.reclaimable.cmp(&a.reclaimable)));

    let root = classified.root.display().to_string();
    for g in &groups {
        let n = g.items.len();
        let first = g.items[0];
        let what = match g.level {
            Level::Never => "kept",
            Level::Safe => "safe to free",
            Level::Review => "to review",
        };
        let title = if n == 1 {
            format!("{}: {} {what}", g.rule_name, human(g.reclaimable))
        } else {
            format!("{}: {} {what} in {n} items", g.rule_name, human(g.reclaimable))
        };
        let mut detail = first.why.clone();
        if !first.regenerate.is_empty() {
            detail.push_str(" — ");
            detail.push_str(&first.regenerate);
        }
        if !first.cost.is_empty() {
            detail.push_str(" — ");
            detail.push_str(&first.cost);
        }
        let mut evidence: Vec<Evidence> = g
            .items
            .iter()
            .take(10)
            .map(|r| Evidence {
                label: format!("{} — {}", r.display, human(if g.level == Level::Never { r.size } else { r.reclaimable })),
                path: r.path.clone(),
                line: None,
            })
            .collect();
        if n > 10 {
            evidence.push(Evidence { label: format!("… and {} more", n - 10), path: None, line: None });
        }
        let mut actions = vec![Action {
            kind: ActionKind::Hint,
            label: match g.level {
                Level::Never => "Shown for size only; Cleanator never proposes it".to_string(),
                _ => "Tick the items in Cleanator and write the script; nothing is removed until you run it".to_string(),
            },
            target: None,
        }];
        if n == 1 {
            if let Some(cmd) = &first.command {
                actions.push(Action { kind: ActionKind::Command, label: "The script would run".into(), target: Some(cmd.clone()) });
            }
        }
        // The folder: the item itself when it is one, its parent otherwise.
        if let Some(p) = first.path.as_deref().map(Path::new) {
            let folder = if p.is_dir() { Some(p) } else { p.parent() };
            if let Some(f) = folder {
                actions.push(Action { kind: ActionKind::OpenFile, label: format!("Open {}", f.display()), target: Some(f.display().to_string()) });
            }
        }
        // A rule's source is a URL most of the time; when it is a sentence, it is a hint.
        if first.source_url.starts_with("http") {
            actions.push(Action { kind: ActionKind::OpenUrl, label: "Why it is regenerable".into(), target: Some(first.source_url.clone()) });
        } else if !first.source_url.is_empty() {
            actions.push(Action { kind: ActionKind::Hint, label: format!("Source: {}", first.source_url), target: None });
        }
        report.findings.push(Finding {
            id: format!("{}/{}", g.level, g.rule_id),
            severity: severity_of(g.level, g.reclaimable),
            kind: g.level.to_string(),
            title,
            subject: Subject { kind: SubjectKind::Path, label: format!("{} · {}", g.category, g.rule_name) },
            detail,
            evidence,
            actions,
            open: Some(Open { app: APP.into(), args: vec![root.clone(), "--select".into(), g.rule_id.to_string()] }),
        });
    }

    let s = &classified.scan;
    report.notes.push(format!("{} files in {} dirs, {} rules, dormancy {} days", s.files, s.dirs, classified.rule_count, classified.dormancy_days));
    let recent = t.counts.get("recent").copied().unwrap_or(0);
    if recent > 0 {
        report.notes.push(format!("{recent} item(s) used within the window, shown and not proposed"));
    }
    if !s.other_filesystems.is_empty() {
        report.notes.push(format!("{} other filesystem(s) not entered", s.other_filesystems.len()));
    }
    if !classified.atime.noatime_mounts.is_empty() {
        report.notes.push(format!("last use unknown on {} (noatime)", classified.atime.noatime_mounts.join(", ")));
    }
    for p in &classified.probes {
        match &p.status {
            crate::probes::ProbeStatus::Ok => {}
            crate::probes::ProbeStatus::ToolMissing(t) => report.notes.push(format!("{}: {t} not installed, not probed", p.probe)),
            crate::probes::ProbeStatus::Failed(e) => report.errors.push(format!("{}: {e}", p.probe)),
            crate::probes::ProbeStatus::NotImplemented(w) => report.notes.push(format!("{}: {w}", p.probe)),
        }
    }
    if s.errors > 0 {
        report.errors.push(format!("{} unreadable director{}", s.errors, if s.errors == 1 { "y" } else { "ies" }));
    }
    report
}
