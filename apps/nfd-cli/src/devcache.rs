//! `nfd devcache [list | clean | unknown]` (PHASE4_DEVCACHE §2.4).
//!
//! `list` matches the DevCache packs and prints them vendor → app → rule with what is
//! deliberately kept (🔒). `clean` is `nfd clean` limited to the DevCache packs (and
//! optionally one vendor): a dry run unless `--apply`, then Planner → SafetyGate →
//! Executor → quarantine + undo journal. `unknown` lists cache-like folders without
//! a rule (never cleaned).

use std::process::ExitCode;

use anyhow::bail;
use clap::{Args, Subcommand};
use nfd_core::RiskLevel;
use nfd_core::safety::SafetyGate;
use nfd_core::units::{format_count, format_size, parse_size};
use nfd_devtools::devcache::{self, DevCacheReport, DevRule};
use nfd_devtools::unknown::{self, UnknownOptions};
use nfd_platform::Platform;
use nfd_rules::matcher::{MatchOptions, Matcher};
use serde_json::json;

use crate::clean::{CleanArgs, Level, load_packs};

/// JSON schema id of `nfd devcache list --json`.
pub const SCHEMA: &str = "nanofreedisk.devcache.v1";
/// JSON schema id of `nfd devcache unknown --json`.
pub const UNKNOWN_SCHEMA: &str = "nanofreedisk.devcache-unknown.v1";

#[derive(Subcommand, Debug)]
pub enum DevcacheAction {
    /// Developer caches grouped vendor → app → folder (the default).
    List(ListArgs),
    /// Clean developer caches (dry run unless --apply); same options as `nfd clean`.
    Clean(DevCleanArgs),
    /// Cache-like folders that no rule knows yet (shown only, never cleaned).
    Unknown(UnknownArgs),
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only this vendor (e.g. `Anthropic`, `JetBrains`).
    #[arg(long)]
    pub vendor: Option<String>,
    /// Highest risk level to list.
    #[arg(long, value_enum, default_value_t = Level::Caution)]
    pub level: Level,
    /// Machine-readable output (`nanofreedisk.devcache.v1`).
    #[arg(long)]
    pub json: bool,
    /// Load rule packs from this folder instead of the built-in ones.
    #[arg(long)]
    pub rules: Option<std::path::PathBuf>,
}

#[derive(Args, Debug)]
pub struct DevCleanArgs {
    /// Only the rules of this vendor.
    #[arg(long)]
    pub vendor: Option<String>,
    #[command(flatten)]
    pub clean: CleanArgs,
}

#[derive(Args, Debug)]
pub struct UnknownArgs {
    /// Smallest folder to report.
    #[arg(long, default_value = "50MB", value_parser = parse_size_arg)]
    pub min_size: u64,
    #[arg(long)]
    pub json: bool,
}

fn parse_size_arg(s: &str) -> Result<u64, String> {
    parse_size(s).map_err(|e| e.to_string())
}

pub fn run(action: Option<DevcacheAction>) -> anyhow::Result<ExitCode> {
    let action = action.unwrap_or(DevcacheAction::List(ListArgs {
        vendor: None,
        level: Level::Caution,
        json: false,
        rules: None,
    }));
    match action {
        DevcacheAction::List(args) => list(&args),
        DevcacheAction::Clean(args) => clean(args),
        DevcacheAction::Unknown(args) => unknown(&args),
    }
}

/// Match the DevCache packs up to `level` and group them.
pub fn report(rules: Option<&std::path::Path>, level: Level) -> anyhow::Result<DevCacheReport> {
    let packs = load_packs(rules)?;
    let dev = devcache::packs(&packs);
    let ids: Vec<String> = dev.iter().map(|p| p.pack.id.clone()).collect();
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    // every pack's 🔒 Keep paths protect the DevCache rules too
    let matcher = Matcher::new(&packs, &resolve, std::env::consts::OS);
    let running = nfd_platform::locks::running_processes().unwrap_or_default();
    let max = level.max();
    let keep = |rule: &nfd_rules::Rule, pack: &str| {
        ids.iter().any(|p| p == pack) && (rule.level <= max || rule.level == RiskLevel::Protected)
    };
    let found = matcher.scan(&keep, &MatchOptions::default(), &running);
    let present = devcache::present_rules(&dev, &resolve);
    Ok(devcache::group(
        &found,
        &ids,
        &devcache::default_thresholds(),
        &present,
    ))
}

fn badge(level: Option<RiskLevel>) -> &'static str {
    match level {
        Some(RiskLevel::Safe) => "[safe]    ",
        Some(RiskLevel::Caution) => "[caution] ",
        Some(RiskLevel::Advanced) => "[advanced]",
        Some(RiskLevel::Protected) | None => "[kept]    ",
    }
}

fn date(t: i64) -> String {
    if t <= 0 {
        return "-".into();
    }
    let (y, m, d, ..) = nfd_core::exec::civil(t);
    format!("{y:04}-{m:02}-{d:02}")
}

fn print_rule(r: &DevRule) {
    let s = &r.summary;
    println!(
        "      {} {:<40} {:>10} {:>9} files  last {}",
        badge(s.level),
        s.rule_id,
        format_size(s.bytes),
        format_count(s.files),
        date(r.newest)
    );
    println!("                 if deleted: {}", s.impact);
    if !s.running.is_empty() {
        println!(
            "                 close first: {} (running; locked files are skipped)",
            s.running.join(", ")
        );
    }
}

fn list(args: &ListArgs) -> anyhow::Result<ExitCode> {
    let mut rep = report(args.rules.as_deref(), args.level)?;
    if let Some(v) = &args.vendor {
        rep.vendors.retain(|x| x.vendor.eq_ignore_ascii_case(v));
        if rep.vendors.is_empty() {
            bail!("no DevCache vendor named `{v}`");
        }
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": SCHEMA, "report": rep }))?
        );
        return Ok(ExitCode::SUCCESS);
    }
    for v in &rep.vendors {
        println!(
            "\n== {}  {} (safe {}){}",
            v.vendor,
            format_size(v.bytes),
            format_size(v.safe_bytes),
            match v.threshold {
                Some(t) if v.over_threshold => format!("  — over the {} threshold", format_size(t)),
                _ => String::new(),
            }
        );
        for a in &v.apps {
            println!("   {:<44} {:>10}", a.app, format_size(a.bytes));
            for r in &a.rules {
                print_rule(r);
            }
            for k in &a.kept {
                println!(
                    "      {} {:<40} {}",
                    badge(None),
                    k.summary.rule_id,
                    k.summary.impact
                );
            }
        }
    }
    println!(
        "\nSafe {}  ·  caution {}  ·  advanced {}  ·  {} kept rule(s)",
        format_size(rep.safe),
        format_size(rep.caution),
        format_size(rep.advanced),
        rep.kept
    );
    println!(
        "Clean the safe ones with `nfd devcache clean --apply` (add `--vendor <name>` for one vendor)."
    );
    Ok(ExitCode::SUCCESS)
}

fn clean(mut args: DevCleanArgs) -> anyhow::Result<ExitCode> {
    let packs = load_packs(args.clean.rules.as_deref())?;
    let dev = devcache::packs(&packs);
    if args.clean.pack.is_empty() {
        args.clean.pack = dev.iter().map(|p| p.pack.id.clone()).collect();
    } else if let Some(bad) = args
        .clean
        .pack
        .iter()
        .find(|p| !dev.iter().any(|d| &d.pack.id == *p))
    {
        bail!("`{bad}` is not a DevCache pack (use `nfd clean` for the others)");
    }
    args.clean.command = Some(match &args.vendor {
        Some(v) => format!("nfd devcache clean --vendor \"{v}\""),
        None => "nfd devcache clean".to_owned(),
    });
    if let Some(vendor) = &args.vendor {
        let ids: Vec<String> = dev
            .iter()
            .flat_map(|p| &p.rules)
            .filter(|r| r.vendor.eq_ignore_ascii_case(vendor))
            .filter(|r| args.clean.rule.is_empty() || args.clean.rule.contains(&r.id))
            .map(|r| r.id.clone())
            .collect();
        if ids.is_empty() {
            bail!("no DevCache rule of vendor `{vendor}`");
        }
        args.clean.rule = ids;
    }
    crate::clean::run(args.clean)
}

fn unknown(args: &UnknownArgs) -> anyhow::Result<ExitCode> {
    let packs = load_packs(None)?;
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    let gate = SafetyGate::new(&resolve);
    let found = unknown::find(
        &packs,
        &resolve,
        &gate,
        UnknownOptions {
            min_bytes: args.min_size,
            ..UnknownOptions::default()
        },
    );
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": UNKNOWN_SCHEMA, "folders": found }))?
        );
        return Ok(ExitCode::SUCCESS);
    }
    if found.is_empty() {
        println!(
            "No unknown cache-like folders of {} or more.",
            format_size(args.min_size)
        );
        return Ok(ExitCode::SUCCESS);
    }
    for f in &found {
        println!(
            "  [unverified] {:>10}{} {:>9} files  last {}  {}",
            format_size(f.totals.bytes),
            if f.totals.truncated { "+" } else { " " },
            format_count(f.totals.files),
            date(f.totals.newest),
            f.path.display()
        );
        println!("               why: {}", f.signals.join(", "));
    }
    println!(
        "\n{} folder(s) without a rule. They are never cleaned automatically; propose a rule with the link in `--json` (propose_url) or https://github.com/congluuit/NANOFreeDisk/issues/new?template=rule_proposal.yml",
        found.len()
    );
    Ok(ExitCode::SUCCESS)
}
