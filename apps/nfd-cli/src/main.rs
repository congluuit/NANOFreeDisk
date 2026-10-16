//! `nfd` — NANOFreeDisk CLI.
//!
//! Ships `scan` (Phase 1), `diff` (scan history, Phase 2), `clean`, `quarantine` and
//! `plan explain` (Phase 3), `devcache`, `toolchains`, `repos`, `stores` (Phase 4), `rules list` / `rules validate`, and the Hardware
//! Monitor's `hw` / `sensors` / `ps`. Dedupe, watch and health commands arrive with
//! their phases (see `docs/PLAN.md` §4.7).

mod clean;
mod devcache;
mod diff;
mod hw;
mod ps;
mod quarantine;
mod repos;
mod scan;
mod sensors;
mod stores;

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
    /// What changed between two saved scans of a folder (`nfd scan --save`, or the app).
    Diff(diff::DiffArgs),
    /// Find junk with the rule packs and clean it (dry run unless --apply).
    Clean(clean::CleanArgs),
    /// Developer caches by vendor → app → folder; clean them; find unknown ones.
    Devcache {
        #[command(subcommand)]
        action: Option<devcache::DevcacheAction>,
    },
    /// Installed toolchains and SDKs, with the proper command to remove each (read-only).
    Toolchains(repos::ToolchainsArgs),
    /// Git repositories and projects under a folder, with their build output; `clean`
    /// moves the build output to the quarantine.
    Repos(repos::ReposArgs),
    /// Package managers' own cache cleanup commands (`npm cache clean`, `pnpm store
    /// prune`, …): list, or run one after confirming the exact command.
    Stores(stores::StoresArgs),
    /// Clean / delete operations: list, restore, purge the quarantine.
    Quarantine {
        #[command(subcommand)]
        action: quarantine::QuarantineAction,
    },
    /// Explain the cleaning plan for a path.
    Plan {
        #[command(subcommand)]
        action: PlanAction,
    },
    /// What is this folder, and what happens if it is deleted? (Folder Encyclopedia)
    Explain {
        /// File or folder to look up (its nearest described parent is used).
        path: PathBuf,
        /// Language of the text: en or vi.
        #[arg(long, default_value = "en")]
        lang: String,
    },
    /// Hardware inventory: CPU, mainboard, memory sticks, GPUs, disks, displays, battery.
    Hw(hw::HwArgs),
    /// Live sensors: load, clocks, temperatures, power, memory, disks, battery.
    Sensors(sensors::SensorsArgs),
    /// Running processes with CPU, memory, I/O and GPU use (read-only).
    Ps(ps::PsArgs),
}

#[derive(Subcommand, Debug)]
enum PlanAction {
    /// Why a path may or may not be deleted, by hand and by each rule that covers it.
    Explain {
        path: PathBuf,
        /// Rule packs folder [default: built-in].
        #[arg(long)]
        rules: Option<PathBuf>,
    },
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
        Command::Diff(args) => diff::run(args),
        Command::Clean(args) => clean::run(args),
        Command::Devcache { action } => devcache::run(action),
        Command::Toolchains(args) => repos::toolchains(&args),
        Command::Repos(args) => repos::repos(args),
        Command::Stores(args) => stores::run(args),
        Command::Quarantine { action } => quarantine::run(action),
        Command::Plan {
            action: PlanAction::Explain { path, rules },
        } => clean::explain(&path, rules.as_deref()),
        Command::Explain { path, lang } => explain(&path, &lang),
        Command::Hw(args) => hw::hw(args),
        Command::Sensors(args) => sensors::run(args),
        Command::Ps(args) => ps::run(args),
    }
}

fn explain(path: &std::path::Path, lang: &str) -> anyhow::Result<ExitCode> {
    let platform = nfd_platform::Native;
    let matcher = nfd_rules::encyclopedia::Matcher::new(
        nfd_rules::encyclopedia::builtin(),
        &platform.resolver(),
    );
    let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let Some(hit) = matcher.lookup(&full, &|p| p.exists()) else {
        println!("No encyclopedia article for {}.", full.display());
        return Ok(ExitCode::SUCCESS);
    };
    let a = hit.article;
    let text = a
        .text
        .get(lang)
        .or_else(|| a.text.get("en"))
        .context("article has no text")?;
    let about = full.ancestors().nth(hit.depth_up).unwrap_or(&full);
    println!("{} [{}]", text.title, a.risk);
    if hit.inside {
        println!("(inside {})", about.display());
    }
    println!("\n{}", text.body.replace("**", "").replace('`', ""));
    Ok(ExitCode::SUCCESS)
}

fn rules(action: RulesAction) -> anyhow::Result<ExitCode> {
    match action {
        RulesAction::Validate { dir } => {
            let packs = nfd_rules::load_dir(&dir)
                .with_context(|| format!("loading rule packs from {}", dir.display()))?;
            let mut findings = nfd_rules::validate(&packs);
            // rules/encyclopedia/*.md (Folder Encyclopedia), when present
            let enc_dir = dir.join("encyclopedia");
            if enc_dir.is_dir() {
                let articles = nfd_rules::encyclopedia::load_dir(&enc_dir)
                    .with_context(|| format!("loading {}", enc_dir.display()))?;
                findings.extend(nfd_rules::encyclopedia::validate(&articles));
                findings.extend(nfd_rules::encyclopedia::validate_rule_links(
                    &articles, &packs,
                ));
                println!("{} encyclopedia article(s)", articles.len());
            }
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
