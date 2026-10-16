//! `nfd clean` and `nfd plan explain` (PHASE3_CLEAN §2.4).
//!
//! `nfd clean` matches the rule packs on this machine and prints what would be freed,
//! grouped by pack and rule. It is a dry run unless `--apply` is given; then the plan
//! goes through the SafetyGate (live checks) and the executor, and every item lands in
//! the quarantine or the Recycle Bin with an entry in the undo journal.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::{Args, ValueEnum};
use nfd_core::RiskLevel;
use nfd_core::plan::{self, CleanPlan, PlanOptions};
use nfd_core::safety::{LiveProbe, Origin, Verdict};
use nfd_core::units::{format_count, format_size};
use nfd_platform::Platform;
use nfd_rules::matcher::{MatchOptions, MatchReport, Matcher};
use serde_json::json;

/// JSON schema id of `nfd clean --json`.
pub const SCHEMA: &str = "nanofreedisk.clean.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Level {
    /// 🟢 only.
    Safe,
    /// 🟢 + 🟡 (each 🟡 rule loses history or forces a re-download).
    Caution,
    /// 🟢 + 🟡 + 🟠 (Expert).
    Advanced,
}

impl Level {
    pub const fn max(self) -> RiskLevel {
        match self {
            Self::Safe => RiskLevel::Safe,
            Self::Caution => RiskLevel::Caution,
            Self::Advanced => RiskLevel::Advanced,
        }
    }
}

#[derive(Args, Debug)]
pub struct CleanArgs {
    /// Packs to use (comma-separated ids, e.g. `windows-system,browsers`); default all.
    #[arg(long, value_delimiter = ',')]
    pub pack: Vec<String>,
    /// Only these rule ids (comma-separated).
    #[arg(long, value_delimiter = ',')]
    pub rule: Vec<String>,
    /// Highest risk level to clean.
    #[arg(long, value_enum, default_value_t = Level::Safe)]
    pub level: Level,
    /// Show what would be cleaned without touching anything (the default).
    #[arg(long, conflicts_with = "apply")]
    pub dry_run: bool,
    /// Clean for real: items go to the quarantine (restorable for 7 days, see
    /// `nfd quarantine`) or, where that is not possible, the Recycle Bin.
    #[arg(long)]
    pub apply: bool,
    /// With --apply and --level safe only: delete permanently instead of
    /// quarantining (asks you to type DELETE).
    #[arg(long, requires = "apply")]
    pub permanent: bool,
    /// Do not ask for confirmation (except for --permanent).
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Machine-readable output (`nanofreedisk.clean.v1`).
    #[arg(long)]
    pub json: bool,
    /// Undo journal / history database used by --apply [default: the desktop app's].
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Load rule packs from this folder instead of the built-in ones.
    #[arg(long)]
    pub rules: Option<PathBuf>,
    /// Also list items too deep to show by default.
    #[arg(long, default_value_t = 20)]
    pub top: usize,
}

pub fn load_packs(dir: Option<&Path>) -> anyhow::Result<Vec<nfd_rules::Pack>> {
    match dir {
        Some(dir) => nfd_rules::load_dir(dir)
            .with_context(|| format!("loading rule packs from {}", dir.display())),
        None => nfd_rules::builtin_packs().context("built-in rule packs"),
    }
}

/// Match the packs on this machine.
pub fn find(args: &CleanArgs) -> anyhow::Result<(MatchReport, CleanPlan)> {
    let packs = load_packs(args.rules.as_deref())?;
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    let matcher = Matcher::new(&packs, &resolve, std::env::consts::OS);
    let running = nfd_platform::locks::running_processes().unwrap_or_default();
    let max = args.level.max();
    let keep = |rule: &nfd_rules::Rule, pack: &str| {
        (args.pack.is_empty() || args.pack.iter().any(|p| p == pack))
            && (args.rule.is_empty() || args.rule.contains(&rule.id))
            && (rule.level <= max || rule.level == RiskLevel::Protected)
    };
    let report = matcher.scan(&keep, &MatchOptions::default(), &running);
    let plan = plan::build(
        report.plan_items(),
        PlanOptions {
            expert: args.level == Level::Advanced,
            include_caution: args.level >= Level::Caution,
            permanent: args.permanent && args.level == Level::Safe,
        },
    );
    Ok((report, plan))
}

impl PartialOrd for Level {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.max().cmp(&other.max()))
    }
}

pub fn run(args: CleanArgs) -> anyhow::Result<ExitCode> {
    let (report, plan) = find(&args)?;
    if args.apply {
        return crate::quarantine::apply(&args, &report, plan);
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&to_json(&report, &plan, true))?
        );
    } else {
        print_report(&report, &plan, args.top);
        println!(
            "\nDry run: nothing was deleted. Run `nfd clean --apply{}` to clean the selected items.",
            match args.level {
                Level::Safe => "",
                Level::Caution => " --level caution",
                Level::Advanced => " --level advanced",
            }
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub fn to_json(report: &MatchReport, plan: &CleanPlan, dry_run: bool) -> serde_json::Value {
    let by_level: serde_json::Map<String, serde_json::Value> = plan
        .bytes_by_level()
        .iter()
        .map(|(l, b)| (l.as_str().to_owned(), json!(b)))
        .collect();
    json!({
        "schema": SCHEMA,
        "dryRun": dry_run,
        "rules": report.rules,
        "items": plan.items.iter().map(|i| json!({
            "path": i.path,
            "ruleId": i.rule_id,
            "level": i.level(),
            "isDir": i.is_dir,
            "bytes": i.bytes,
            "files": i.files,
            "mode": i.mode,
            "selected": i.selected,
        })).collect::<Vec<_>>(),
        "totals": {
            "selectedBytes": plan.selected_bytes(),
            "selectedItems": plan.selected().count(),
            "byLevel": by_level,
            "byPack": report.bytes_by_pack(),
        },
    })
}

fn badge(level: Option<RiskLevel>) -> &'static str {
    match level {
        Some(RiskLevel::Safe) => "[safe]    ",
        Some(RiskLevel::Caution) => "[caution] ",
        Some(RiskLevel::Advanced) => "[advanced]",
        Some(RiskLevel::Protected) | None => "[guide]   ",
    }
}

fn print_report(report: &MatchReport, plan: &CleanPlan, top: usize) {
    let mut pack = String::new();
    for r in &report.rules {
        if r.pack_id != pack {
            pack.clone_from(&r.pack_id);
            println!("\n== {pack}");
        }
        if r.level == Some(RiskLevel::Protected) {
            println!("  {} {:<40} {}", badge(r.level), r.rule_id, r.impact);
            continue;
        }
        if r.candidates == 0 {
            continue;
        }
        let selected = plan
            .items
            .iter()
            .any(|i| i.selected && i.rule_id.as_deref() == Some(r.rule_id.as_str()));
        println!(
            "  {} {:<40} {:>10}  {:>9} files{}",
            badge(r.level),
            r.rule_id,
            format_size(r.bytes),
            format_count(r.files),
            if selected { "" } else { "  (not selected)" }
        );
        if !r.running.is_empty() {
            println!(
                "             close first: {} (locked files are skipped)",
                r.running.join(", ")
            );
        }
    }
    let mut items: Vec<_> = plan.items.iter().collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    if !items.is_empty() && top > 0 {
        println!("\nLargest items:");
        for i in items.iter().take(top) {
            println!(
                "  {:>10}  {}{}",
                format_size(i.bytes),
                i.path.display(),
                if i.selected { "" } else { "  (not selected)" }
            );
        }
    }
    println!(
        "\nWould free {} in {} item(s).",
        format_size(plan.selected_bytes()),
        format_count(plan.selected().count() as u64)
    );
}

/// `nfd plan explain <path>`: why a path may or may not be deleted.
pub fn explain(path: &Path, rules_dir: Option<&Path>) -> anyhow::Result<ExitCode> {
    let packs = load_packs(rules_dir)?;
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    let matcher = Matcher::new(&packs, &resolve, std::env::consts::OS);
    let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let gate = matcher.gate();
    println!("{}", full.display());
    let show = |label: &str, origin: Origin| {
        let lexical = gate.check_path(&full, origin);
        let live = gate.check_live(&full, origin, None, &LiveProbe);
        let verdict = match (lexical, live) {
            (Err(b), _) | (Ok(()), Err(b)) => Verdict::Blocked(b),
            (Ok(()), Ok(w)) if w.is_empty() => Verdict::Allowed,
            (Ok(()), Ok(w)) => Verdict::Warn(w),
        };
        match verdict {
            Verdict::Allowed => println!("  {label}: allowed"),
            Verdict::Warn(w) => println!("  {label}: allowed with warnings {w:?}"),
            Verdict::Blocked(b) => println!("  {label}: BLOCKED ({}) — {b}", b.code()),
        }
    };
    show("picked by hand", Origin::Manual);
    let hits = matcher.rules_matching(&full);
    if hits.is_empty() {
        println!("  no rule covers this path");
    }
    for (rule, pack) in hits {
        show(
            &format!("rule {pack}/{} ({})", rule.id, rule.level),
            Origin::Rule { level: rule.level },
        );
    }
    Ok(ExitCode::SUCCESS)
}
