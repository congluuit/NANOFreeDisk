//! `nfd toolchains` (C2) and `nfd repos [clean] <root>` (C3 + C9), PHASE4_DEVCACHE §2.4.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use nfd_core::RiskLevel;
use nfd_core::safety::SafetyGate;
use nfd_core::units::{format_count, format_size};
use nfd_devtools::process;
use nfd_devtools::repos::{self, RepoOptions, RepoScan};
use nfd_devtools::toolchains::{self, Guide, Sources};
use nfd_platform::Platform;
use serde_json::json;

/// JSON schema id of `nfd toolchains --json`.
pub const TOOLCHAINS_SCHEMA: &str = "nanofreedisk.toolchains.v1";
/// JSON schema id of `nfd repos --json`.
pub const REPOS_SCHEMA: &str = "nanofreedisk.repos.v1";

#[derive(Args, Debug)]
pub struct ToolchainsArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub struct ReposArgs {
    #[command(subcommand)]
    pub action: Option<ReposAction>,
    #[command(flatten)]
    pub find: FindArgs,
}

#[derive(Subcommand, Debug)]
pub enum ReposAction {
    /// Move the build output of the projects under <root> to the quarantine
    /// (dry run unless --apply).
    Clean(CleanReposArgs),
}

#[derive(Args, Debug, Clone)]
pub struct FindArgs {
    /// Folder to search for git repositories and projects.
    pub root: Option<PathBuf>,
    /// Keep (do not pre-select) the artifacts of projects active in the last N days
    /// (0 = off).
    #[arg(long, default_value_t = 7)]
    pub keep_recent: u32,
    /// Flag projects without activity for N days as stale.
    #[arg(long, default_value_t = 90)]
    pub stale: u32,
    /// Deepest folder level searched below <root>.
    #[arg(long, default_value_t = 8)]
    pub depth: usize,
    /// Do not run `git status` for each repository.
    #[arg(long)]
    pub no_git_status: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct CleanReposArgs {
    #[command(flatten)]
    pub find: FindArgs,
    /// Also clean 🟡 artifacts (Unity Library, Unreal DerivedDataCache, .venv, large
    /// Rust targets).
    #[arg(long)]
    pub include_caution: bool,
    /// Also clean the artifacts of recently active projects.
    #[arg(long)]
    pub include_recent: bool,
    /// Clean for real (quarantine, restorable for 7 days).
    #[arg(long)]
    pub apply: bool,
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Undo journal / history database [default: the desktop app's].
    #[arg(long)]
    pub db: Option<PathBuf>,
}

fn date(t: i64) -> String {
    if t <= 0 {
        return "-".into();
    }
    let (y, m, d, ..) = nfd_core::exec::civil(t);
    format!("{y:04}-{m:02}-{d:02}")
}

const fn guide_text(g: Guide) -> &'static str {
    match g {
        Guide::WindowsApps => "Settings → Apps → Installed apps",
        Guide::UnityHub => "Unity Hub → Installs",
        Guide::EpicLauncher => "Epic Games Launcher → Unreal Engine → Library",
        Guide::VisualStudioInstaller => "Visual Studio Installer",
        Guide::Ide => "the IDE / tool that downloaded it",
        Guide::CompactVhd => {
            "wsl --shutdown, then Optimize-VHD -Mode Full (or diskpart compact vdisk)"
        }
    }
}

/// `nfd toolchains`.
pub fn toolchains(args: &ToolchainsArgs) -> anyhow::Result<ExitCode> {
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    let env = |k: &str| std::env::var(k).ok();
    let sources = Sources {
        resolve: &resolve,
        env: &env,
        vswhere: toolchains::run_vswhere(&resolve),
    };
    let mut list = toolchains::inventory(&sources, 2_000_000);
    list.sort_by(|a, b| {
        a.family
            .cmp(b.family)
            .then(b.totals.bytes.cmp(&a.totals.bytes))
    });
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({ "schema": TOOLCHAINS_SCHEMA, "toolchains": list })
            )?
        );
        return Ok(ExitCode::SUCCESS);
    }
    let mut family = "";
    for t in &list {
        if t.family != family {
            family = t.family;
            let total: u64 = list
                .iter()
                .filter(|x| x.family == family)
                .map(|x| x.totals.bytes)
                .sum();
            println!("\n== {family}  {}", format_size(total));
        }
        println!(
            "  {}{:<44} {:>10}{} last {}",
            if t.default { "* " } else { "  " },
            t.name,
            format_size(t.totals.bytes),
            if t.totals.truncated { "+" } else { " " },
            date(t.totals.newest)
        );
        match (&t.command, t.guide) {
            (Some(cmd), _) => println!("      remove: {cmd}"),
            (None, Some(g)) => println!("      remove: {}", guide_text(g)),
            (None, None) => {}
        }
    }
    if list.is_empty() {
        println!("No toolchains found.");
    } else {
        println!(
            "\n* = default / in use. NANOFreeDisk never deletes toolchain folders itself: use the command shown."
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn find(args: &FindArgs) -> anyhow::Result<RepoScan> {
    let root = match &args.root {
        Some(r) => std::path::absolute(r)?,
        None => std::env::current_dir()?,
    };
    anyhow::ensure!(root.is_dir(), "{} is not a folder", root.display());
    let gate = SafetyGate::new(&nfd_platform::Native.resolver());
    let git = process::git();
    let opts = RepoOptions {
        max_depth: args.depth,
        keep_recent_days: args.keep_recent,
        stale_days: args.stale,
        git_status: !args.no_git_status,
        ..RepoOptions::default()
    };
    Ok(repos::find(&root, opts, git.as_deref(), &gate, None))
}

fn badge(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::Safe => "[safe]   ",
        RiskLevel::Caution => "[caution]",
        RiskLevel::Advanced | RiskLevel::Protected => "[guide]  ",
    }
}

fn print_scan(scan: &RepoScan) {
    for r in &scan.repos {
        println!(
            "\n== {}  {}  .git {}  artifacts {}{}{}{}",
            r.name,
            r.path.display(),
            format_size(r.git_bytes),
            format_size(r.artifact_bytes),
            if r.git { "" } else { "  (no git)" },
            match r.dirty {
                Some(true) => "  · uncommitted changes",
                _ => "",
            },
            if r.recent {
                "  · active recently (kept)"
            } else if r.stale {
                "  · stale"
            } else {
                ""
            }
        );
        if let Some(ts) = r.last_commit {
            println!(
                "   last commit {}: {}",
                date(ts),
                r.last_commit_subject.as_deref().unwrap_or("")
            );
        }
        for a in &r.artifacts {
            println!(
                "   {} {} {:<36} {:>10} {:>9} files{}",
                if a.selected { "x" } else { " " },
                badge(a.level),
                a.rel,
                format_size(a.totals.bytes),
                format_count(a.totals.files),
                a.blocked
                    .as_deref()
                    .map(|b| format!("  (not offered: {b})"))
                    .unwrap_or_default()
            );
        }
    }
    let selected: u64 = scan
        .repos
        .iter()
        .flat_map(|r| &r.artifacts)
        .filter(|a| a.selected)
        .map(|a| a.totals.bytes)
        .sum();
    println!(
        "\n{} project(s) in {} folder(s); {} of build output pre-selected (x).{}",
        scan.repos.len(),
        format_count(scan.folders),
        format_size(selected),
        if scan.git {
            ""
        } else {
            " git was not found: generic folders (build, dist, bin, obj) are not offered."
        }
    );
}

/// `nfd repos [clean] <root>`.
pub fn repos(args: ReposArgs) -> anyhow::Result<ExitCode> {
    match args.action {
        None => {
            let scan = find(&args.find)?;
            if args.find.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({ "schema": REPOS_SCHEMA, "scan": scan }))?
                );
            } else {
                print_scan(&scan);
                println!("Clean them with `nfd repos clean <root> --apply`.");
            }
            Ok(ExitCode::SUCCESS)
        }
        Some(ReposAction::Clean(c)) => clean(&c),
    }
}

fn clean(args: &CleanReposArgs) -> anyhow::Result<ExitCode> {
    let scan = find(&args.find)?;
    let picked: Vec<&repos::Artifact> = scan
        .repos
        .iter()
        .flat_map(|r| r.artifacts.iter().map(move |a| (r, a)))
        .filter(|(r, a)| {
            a.blocked.is_none()
                && (a.selected
                    || ((args.include_recent || !r.recent)
                        && (a.level == RiskLevel::Safe
                            || (args.include_caution && a.level == RiskLevel::Caution))))
        })
        .map(|(_, a)| a)
        .collect();
    let plan = repos::plan(picked.iter().copied());
    if !args.find.json {
        print_scan(&scan);
    }
    if plan.items.is_empty() {
        println!("Nothing to clean.");
        return Ok(ExitCode::SUCCESS);
    }
    if !args.apply {
        println!(
            "\nDry run: would move {} in {} folder(s) to the quarantine. Add --apply to do it.",
            format_size(plan.selected_bytes()),
            plan.items.len()
        );
        return Ok(ExitCode::SUCCESS);
    }
    if !crate::quarantine::confirm(&plan, args.yes)? {
        return Ok(ExitCode::FAILURE);
    }
    let (op, journal_ok) =
        crate::quarantine::execute(&plan, "devcache", args.db.clone(), args.find.json);
    if args.find.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": REPOS_SCHEMA, "operation": op }))?
        );
    } else {
        crate::quarantine::print_outcome(&op, journal_ok);
    }
    Ok(crate::quarantine::exit_code(&op))
}
