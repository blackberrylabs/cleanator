//! `cleanator-cli`: the core without a window. Scans, classifies, writes the script.
//! It never deletes anything (R1): `script` only prints or saves a file for you to read.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use cleanator_core::classify::{classify, ClassifyOptions, Report};
use cleanator_core::rules::{Level, RuleSet};
use cleanator_core::scan::{scan, Scan, ScanOptions};
use cleanator_core::script::{generate, ScriptOptions, Selection};
use cleanator_core::size::human;

#[derive(Parser)]
#[command(
    name = "cleanator-cli",
    version,
    about = "Disk cleaner for AI developers on Linux — core CLI. Never deletes anything."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List and validate the rules (embedded, or from a directory).
    Rules {
        /// Directory of rule files (default: the rules embedded in the binary).
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Walk a directory and report sizes the way `du -x` does.
    Scan {
        path: PathBuf,
        #[arg(long)]
        json: bool,
        /// Depth of the printed tree.
        #[arg(long, default_value_t = 2)]
        depth: usize,
        /// Also cross into other filesystems (default: stay on the root's, like `du -x`).
        #[arg(long)]
        all_filesystems: bool,
    },
    /// Scan, apply the rules and list the recommendations.
    Recommend {
        path: PathBuf,
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        /// Days without use before something counts as dormant.
        #[arg(long, default_value_t = 60)]
        dormancy_days: u32,
        /// Skip the tool probes (docker, ollama, snap…).
        #[arg(long)]
        no_probes: bool,
        /// Skip the in-use checks (running processes, open files, bind mounts).
        #[arg(long)]
        no_in_use_checks: bool,
        /// Show every item including `never` ones and rules that found nothing.
        #[arg(long)]
        explain: bool,
        #[arg(long)]
        all_filesystems: bool,
    },
    /// Scan, apply the rules and write the cleanup script (printed, or saved with -o).
    Script {
        path: PathBuf,
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Which items: safe (default), all (safe + review), or explicit ids with --select.
        #[arg(long, value_enum, default_value_t = Which::Safe)]
        which: Which,
        /// Comma-separated recommendation ids (`rule-id::key`), implies --which ids.
        #[arg(long, value_delimiter = ',')]
        select: Vec<String>,
        #[arg(long, default_value_t = 60)]
        dormancy_days: u32,
        #[arg(long)]
        no_probes: bool,
        #[arg(long)]
        no_in_use_checks: bool,
        /// Write the script here instead of printing it.
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        all_filesystems: bool,
    },
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum Which {
    Safe,
    All,
    Ids,
}

fn load_rules(dir: Option<&PathBuf>) -> Result<RuleSet> {
    match dir {
        Some(d) => {
            RuleSet::from_dir(d).with_context(|| format!("loading rules from {}", d.display()))
        }
        None => RuleSet::embedded().context("embedded rules"),
    }
}

fn do_scan(path: &Path, all_filesystems: bool) -> Result<Scan> {
    let opts = ScanOptions {
        one_filesystem: !all_filesystems,
        ..Default::default()
    };
    eprintln!("scanning {} …", path.display());
    let scan = scan(path, opts).with_context(|| format!("scanning {}", path.display()))?;
    eprintln!(
        "scanned {} in {} dirs, {} files, {:.1}s ({} unreadable, {} other filesystems skipped)",
        human(scan.summary.total_size),
        scan.summary.dirs,
        scan.summary.files,
        scan.summary.duration_ms as f64 / 1000.0,
        scan.summary.errors,
        scan.summary.other_filesystems.len()
    );
    Ok(scan)
}

fn do_report(
    path: &Path,
    rules: Option<&PathBuf>,
    dormancy_days: u32,
    no_probes: bool,
    no_in_use: bool,
    all_fs: bool,
) -> Result<(Scan, Report)> {
    let rules = load_rules(rules)?;
    let scan = do_scan(path, all_fs)?;
    let env = cleanator_core::paths::Env::current();
    let opts = ClassifyOptions {
        dormancy_days,
        run_probes: !no_probes,
        in_use_checks: !no_in_use,
        ..Default::default()
    };
    eprintln!(
        "applying {} rules{} …",
        rules.len(),
        if no_probes { " (no probes)" } else { "" }
    );
    let report = classify(&scan, &rules, &env, &opts);
    Ok((scan, report))
}

fn print_tree(node: &cleanator_core::scan::DirNode, depth: usize, indent: usize) {
    println!(
        "{:>8}  {}{}",
        human(node.size),
        "  ".repeat(indent),
        if indent == 0 {
            node.path.display().to_string()
        } else {
            node.name.clone()
        }
    );
    if depth == 0 {
        return;
    }
    for c in node.children.iter().take(25) {
        if c.size > 0 || c.other_fs {
            print_tree(c, depth - 1, indent + 1);
        }
    }
}

fn date(ts: Option<i64>) -> String {
    ts.map(cleanator_core::size::ymd)
        .unwrap_or_else(|| "—".into())
}

fn print_report(report: &Report, explain: bool) {
    let t = &report.totals;
    println!("== Cleanator — {} ==", report.root.display());
    println!(
        "scanned {}   safe {}   review {}   in use {}   never {}",
        human(report.scan.total_size),
        human(t.safe),
        human(t.review),
        human(t.in_use),
        human(t.never)
    );
    println!();
    for level in [Level::Safe, Level::Review] {
        let items: Vec<_> = report
            .recommendations
            .iter()
            .filter(|r| r.effective_level == level && !r.recent)
            .collect();
        if items.is_empty() {
            continue;
        }
        println!("-- {} ({} items) --", level, items.len());
        for r in items {
            let flag = match &r.in_use {
                Some(reason) => format!("  [in use: {reason}]"),
                None => String::new(),
            };
            println!(
                "{:>8}  {:<28} {}  (last used {}){}",
                human(r.reclaimable),
                r.rule_id,
                r.display,
                date(r.last_used),
                flag
            );
            if explain {
                println!("          why: {}", r.why);
                if let Some(c) = &r.command {
                    println!("          cmd: {c}");
                }
                if let Some(n) = &r.notes {
                    println!("          note: {n}");
                }
            }
        }
        println!();
    }
    if explain {
        let recent: Vec<_> = report.recommendations.iter().filter(|r| r.recent).collect();
        println!(
            "-- recent ({} items classified but used within {} days, not proposed) --",
            recent.len(),
            report.dormancy_days
        );
        for r in recent.iter().take(60) {
            println!(
                "{:>8}  {:<28} {}  (last used {})",
                human(r.size),
                r.rule_id,
                r.display,
                date(r.last_used)
            );
        }
        println!();
        let never: Vec<_> = report
            .recommendations
            .iter()
            .filter(|r| r.effective_level == Level::Never)
            .collect();
        println!("-- never ({} items, shown only) --", never.len());
        for r in never.iter().take(60) {
            println!("{:>8}  {:<28} {}", human(r.size), r.rule_id, r.display);
        }
        println!();
    }
    let failed: Vec<_> = report
        .probes
        .iter()
        .filter(|p| p.status != cleanator_core::probes::ProbeStatus::Ok)
        .collect();
    if !failed.is_empty() {
        println!("-- not scanned --");
        for p in failed {
            println!(
                "  {}{}: {:?}",
                p.probe,
                p.item.as_ref().map(|i| format!("/{i}")).unwrap_or_default(),
                p.status
            );
        }
        println!();
    }
    if explain {
        println!("-- rules with nothing to report --");
        for n in &report.rule_notes {
            println!("  {n}");
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Rules { rules, json } => {
            let set = load_rules(rules.as_ref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&set)?);
            } else {
                println!("{} rules from {}", set.len(), set.origin);
                for r in &set.rules {
                    println!(
                        "{:<34} {:<7} {:<20} {}{}",
                        r.id,
                        r.level,
                        r.category,
                        r.name,
                        if r.source_to_verify() {
                            "  [source to verify]"
                        } else {
                            ""
                        }
                    );
                }
                let tv = set.rules.iter().filter(|r| r.source_to_verify()).count();
                println!("\n{tv} rules still have a source to verify before release.");
            }
        }
        Command::Scan {
            path,
            json,
            depth,
            all_filesystems,
        } => {
            let scan = do_scan(&path, all_filesystems)?;
            if json {
                let pruned = Scan {
                    summary: scan.summary.clone(),
                    tree: scan.tree.pruned(depth),
                };
                println!("{}", serde_json::to_string_pretty(&pruned)?);
            } else {
                print_tree(&scan.tree, depth, 0);
                if !scan.summary.other_filesystems.is_empty() {
                    println!("\nnot entered (other filesystems):");
                    for p in &scan.summary.other_filesystems {
                        println!("  {}", p.display());
                    }
                }
            }
        }
        Command::Recommend {
            path,
            rules,
            json,
            dormancy_days,
            no_probes,
            no_in_use_checks,
            explain,
            all_filesystems,
        } => {
            let (_, report) = do_report(
                &path,
                rules.as_ref(),
                dormancy_days,
                no_probes,
                no_in_use_checks,
                all_filesystems,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_report(&report, explain);
            }
        }
        Command::Script {
            path,
            rules,
            which,
            select,
            dormancy_days,
            no_probes,
            no_in_use_checks,
            output,
            all_filesystems,
        } => {
            let (_, report) = do_report(
                &path,
                rules.as_ref(),
                dormancy_days,
                no_probes,
                no_in_use_checks,
                all_filesystems,
            )?;
            let selection = if !select.is_empty() || which == Which::Ids {
                Selection::Ids(select)
            } else if which == Which::All {
                Selection::All
            } else {
                Selection::Safe
            };
            let (text, n) = generate(
                &report,
                &ScriptOptions {
                    selection,
                    include_in_use: false,
                },
            );
            match output {
                Some(out) => {
                    std::fs::write(&out, &text)
                        .with_context(|| format!("writing {}", out.display()))?;
                    eprintln!(
                        "wrote {} ({n} items). Read it, then run: bash {}",
                        out.display(),
                        out.display()
                    );
                }
                None => print!("{text}"),
            }
        }
    }
    Ok(())
}
