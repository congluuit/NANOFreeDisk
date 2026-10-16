//! `nfd` — NANOFreeDisk CLI.
//!
//! Ships `scan` (Phase 1) and `rules list` / `rules validate`. Cleaning, dedupe, watch
//! and health commands arrive with their phases (see `docs/PLAN.md` §4.7).

mod scan;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use clap::{Parser, Subcommand};
use nfd_platform::Platform;

#[derive(Parser, Debug)]
#[command(
    name = "nfd",
    version,
    about = "NANOFreeDisk — disk analyzer, safe cleaner, dev-cache manager",
    long_about = None,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Inspect and validate rule packs.
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },
    /// Scan a path and report what uses the space (never follows junctions or symlinks).
    Scan(scan::ScanArgs),
}

#[derive(Subcommand, Debug)]
enum RulesAction {
    /// Validate every *.toml pack in a directory (exit 1 on errors).
    Validate {
        /// Directory containing rule packs.
        dir: PathBuf,
    },
    /// List rules with their risk level and expanded first path.
    List {
        /// Directory containing rule packs.
        dir: PathBuf,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Rules { action } => rules(action),
        Command::Scan(args) => scan::run(args),
    }
}

fn rules(action: RulesAction) -> anyhow::Result<ExitCode> {
    match action {
        RulesAction::Validate { dir } => {
            let packs = nfd_rules::load_dir(&dir)
                .with_context(|| format!("loading rule packs from {}", dir.display()))?;
            let findings = nfd_rules::validate(&packs);
            for f in &findings {
                println!("{f}");
            }
            let rules: usize = packs.iter().map(|p| p.rules.len()).sum();
            let errors = findings.iter().filter(|f| f.error).count();
            let warnings = findings.len() - errors;
            println!(
                "{} pack(s), {rules} rule(s): {errors} error(s), {warnings} warning(s)",
                packs.len()
            );
            Ok(if errors == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        RulesAction::List { dir } => {
            let packs = nfd_rules::load_dir(&dir)
                .with_context(|| format!("loading rule packs from {}", dir.display()))?;
            let platform = nfd_platform::Native;
            let resolve = platform.resolver();
            for pack in &packs {
                println!(
                    "== {} ({}) v{}",
                    pack.pack.name, pack.pack.id, pack.pack.version
                );
                for r in &pack.rules {
                    let first = r.paths.first().map(String::as_str).unwrap_or("-");
                    let expanded = nfd_rules::expand(first, &resolve)
                        .unwrap_or_else(|| "<unresolved on this platform>".into());
                    println!("  {:<9} {:<48} {expanded}", r.level.to_string(), r.id);
                }
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
