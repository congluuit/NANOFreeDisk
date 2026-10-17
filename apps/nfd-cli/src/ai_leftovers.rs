//! `nfd ai-leftovers [list|clean]` (PLAN §13.2 C10, `docs/AI_LEFTOVERS.md` §2.5):
//! what AI coding agents left behind per session, by project → tool → session.
//! `clean` takes the pre-selected items (finished sessions, 🟢 only); it is a dry run
//! unless `--apply`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use nfd_core::RiskLevel;
use nfd_core::exec::{self, ItemStatus};
use nfd_core::safety::SafetyGate;
use nfd_core::units::format_size;
use nfd_devtools::ai_leftovers::{self, Options, Report, Sources, worktrees};
use nfd_devtools::process;
use nfd_platform::Platform;
use serde_json::json;

/// JSON schema id of `nfd ai-leftovers --json`.
pub const SCHEMA: &str = "nanofreedisk.ai-leftovers.v1";

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub struct AiArgs {
    #[command(subcommand)]
    pub action: Option<AiAction>,
    #[command(flatten)]
    pub find: FindArgs,
}

#[derive(Subcommand, Debug)]
pub enum AiAction {
    /// List the leftovers (default).
    List(FindArgs),
    /// Clean the leftovers of finished sessions (🟢, pre-selected): folders to the
    /// quarantine, worktrees with `git worktree remove` (dry run unless --apply).
    Clean(CleanArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct FindArgs {
    /// More folders to look for git repositories in (besides the sessions' folders).
    #[arg(long)]
    pub root: Vec<PathBuf>,
    /// Keep (do not pre-select) leftovers of sessions active in the last N days.
    #[arg(long, default_value_t = 7)]
    pub keep_recent: u32,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct CleanArgs {
    #[command(flatten)]
    pub find: FindArgs,
    /// Clean for real (quarantine, restorable for 7 days; worktrees removed by git).
    #[arg(long)]
    pub apply: bool,
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Undo journal / history database [default: the desktop app's].
    #[arg(long)]
    pub db: Option<PathBuf>,
}

fn find(args: &FindArgs) -> anyhow::Result<Report> {
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    let gate = SafetyGate::new(&resolve);
    let env = |k: &str| std::env::var(k).ok();
    let sources = Sources::detect(&resolve, &env);
    let mut roots = Vec::new();
    for r in &args.root {
        roots.push(std::path::absolute(r)?);
    }
    let opts = Options {
        keep_recent_days: args.keep_recent,
        roots,
        ..Options::default()
    };
    let git = process::git();
    Ok(ai_leftovers::scan(
        &opts,
        &sources,
        git.as_deref(),
        &gate,
        None,
    ))
}

fn badge(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::Safe => "[safe]   ",
        _ => "[caution]",
    }
}

fn days_ago(now: i64, t: i64) -> String {
    if t <= 0 {
        return "?".into();
    }
    let d = (now - t).max(0) / 86_400;
    if d == 0 {
        "today".into()
    } else {
        format!("{d} d ago")
    }
}

fn print(report: &Report) {
    let now = exec::unix_now();
    let mut project: Option<&std::path::Path> = None;
    let mut first = true;
    for it in &report.items {
        if first || it.project.as_deref() != project {
            first = false;
            project = it.project.as_deref();
            println!(
                "\n== {}",
                project.map_or_else(
                    || "(Temp / tool stores)".to_owned(),
                    |p| p.display().to_string()
                )
            );
        }
        let session = it.session.as_ref().map(|s| {
            format!(
                "  · session \"{}\", active {}{}",
                s.title.as_deref().unwrap_or(&s.id),
                days_ago(now, s.last_active),
                if s.open { ", open" } else { "" }
            )
        });
        println!(
            "   {} {} {:<18} {:<44} {:>10}{}",
            if it.selected { "x" } else { " " },
            badge(it.level),
            it.tool,
            it.rel,
            format_size(it.totals.bytes),
            session.unwrap_or_default()
        );
        if let Some(why) = &it.blocked {
            println!("        not offered: {why}");
        } else if let Some(k) = it.keep {
            println!("        kept (not pre-selected): {k}");
        }
        if let Some(wt) = &it.worktree {
            if wt.registered && !wt.prunable {
                println!(
                    "        branch {} · {} · {} change(s)",
                    wt.branch.as_deref().unwrap_or("(detached)"),
                    match (wt.merged, wt.pushed) {
                        (true, _) => "merged",
                        (false, true) => "pushed",
                        _ => "not merged or pushed",
                    },
                    wt.changes
                );
            }
            for a in wt.artifacts.iter().filter(|a| a.blocked.is_none()) {
                println!(
                    "      {} build output {:<34} {:>10}",
                    if a.selected { "x" } else { " " },
                    a.rel,
                    format_size(a.totals.bytes)
                );
            }
        }
    }
    for k in &report.kept {
        println!("   🔒 kept: {} ({}) {}", k.what, k.tool, k.path.display());
    }
    println!(
        "\n{} session(s), {} project(s); {} pre-selected of {} offered (x = finished sessions, 🟢).{}",
        report.sessions,
        report.projects.len(),
        format_size(report.selected_bytes),
        format_size(report.total_bytes),
        if report.git {
            ""
        } else {
            " git was not found: worktrees and common folder names are not offered."
        }
    );
}

/// `nfd ai-leftovers`.
pub fn run(args: AiArgs) -> anyhow::Result<ExitCode> {
    match args.action {
        None => list(&args.find),
        Some(AiAction::List(f)) => list(&f),
        Some(AiAction::Clean(c)) => clean(&c),
    }
}

fn list(args: &FindArgs) -> anyhow::Result<ExitCode> {
    let report = find(args)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": SCHEMA, "report": report }))?
        );
    } else {
        print(&report);
        println!("Clean the pre-selected ones with `nfd ai-leftovers clean --apply`.");
    }
    Ok(ExitCode::SUCCESS)
}

fn clean(args: &CleanArgs) -> anyhow::Result<ExitCode> {
    let report = find(&args.find)?;
    let prepared = ai_leftovers::prepare(&report, &ai_leftovers::preselected(&report));
    if !args.find.json {
        print(&report);
    }
    if prepared.plan.items.is_empty() && prepared.jobs.is_empty() {
        println!("Nothing to clean.");
        return Ok(ExitCode::SUCCESS);
    }
    if !args.apply {
        if args.find.json {
            let commands: Vec<String> = prepared.jobs.iter().map(|j| j.command_line()).collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema": SCHEMA,
                    "dryRun": true,
                    "plan": prepared.plan,
                    "commands": commands,
                }))?
            );
            return Ok(ExitCode::SUCCESS);
        }
        println!(
            "\nDry run: would move {} in {} folder(s) to the quarantine{}. Add --apply to do it.",
            format_size(prepared.plan.selected_bytes()),
            prepared.plan.items.len(),
            if prepared.jobs.is_empty() {
                String::new()
            } else {
                format!(", then run {} git command(s)", prepared.jobs.len())
            }
        );
        for j in &prepared.jobs {
            println!("   {}", j.command_line());
        }
        return Ok(ExitCode::SUCCESS);
    }
    for j in &prepared.jobs {
        eprintln!("will run: {}", j.command_line());
    }
    if !prepared.plan.items.is_empty() && !crate::quarantine::confirm(&prepared.plan, args.yes)? {
        return Ok(ExitCode::FAILURE);
    }
    let mut failed = false;
    let mut ops = Vec::new();
    if !prepared.plan.items.is_empty() {
        let (op, journal_ok) = crate::quarantine::execute(
            &prepared.plan,
            "ai-leftovers",
            args.db.clone(),
            args.find.json,
        );
        if !args.find.json {
            crate::quarantine::print_outcome(&op, journal_ok);
        }
        failed |= op.count(ItemStatus::Failed) > 0;
        ops.push(op);
    }
    if !prepared.jobs.is_empty() {
        let git = process::git().ok_or_else(|| anyhow::anyhow!("git was not found"))?;
        let started = exec::unix_now();
        let items: Vec<_> = prepared
            .jobs
            .iter()
            .enumerate()
            .map(|(n, j)| worktrees::remove(&git, j, n, None))
            .collect();
        let op = worktrees::operation(items, exec::new_op_id(started), started, exec::unix_now());
        let journal_ok = crate::quarantine::record(&op, args.db.clone());
        if !args.find.json {
            for i in &op.items {
                println!(
                    "  {:<9} {}{}",
                    i.status.as_str(),
                    i.path.display(),
                    i.reason
                        .as_deref()
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default()
                );
            }
            if !journal_ok {
                println!("warning: the git commands could not be recorded in the activity log.");
            }
        }
        failed |= op.count(ItemStatus::Failed) > 0;
        ops.push(op);
    }
    if args.find.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": SCHEMA, "operations": ops }))?
        );
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}
