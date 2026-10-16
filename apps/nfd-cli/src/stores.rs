//! `nfd stores [run <id>]` (C4, PHASE4_DEVCACHE §2.4): the package managers' own
//! cleanup commands. Guide-only commands (nothing to download again) are printed,
//! never run; the others run after the exact command line is shown and confirmed.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use nfd_core::exec;
use nfd_core::units::format_size;
use nfd_devtools::process;
use nfd_devtools::stores::{self, StoreInfo, Undo};
use nfd_platform::Platform;
use serde_json::json;

/// JSON schema id of `nfd stores --json`.
pub const SCHEMA: &str = "nanofreedisk.stores.v1";
/// Longest a cleanup command may run.
pub const TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub struct StoresArgs {
    #[command(subcommand)]
    pub action: Option<StoresAction>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum StoresAction {
    /// Run one tool's cleanup command (shown and confirmed first).
    Run {
        /// Command id from `nfd stores` (e.g. `npm-cache-clean`).
        id: String,
        /// Do not ask (the command line is still printed).
        #[arg(long, short = 'y')]
        yes: bool,
        /// History database for the Activity log [default: the desktop app's].
        #[arg(long)]
        db: Option<PathBuf>,
    },
}

fn infos() -> Vec<StoreInfo> {
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    stores::list(&resolve, &process::which, 2_000_000)
}

pub fn run(args: StoresArgs) -> anyhow::Result<ExitCode> {
    match args.action {
        None => list(args.json),
        Some(StoresAction::Run { id, yes, db }) => run_one(&id, yes, db),
    }
}

fn list(json: bool) -> anyhow::Result<ExitCode> {
    let list = infos();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": SCHEMA, "stores": list }))?
        );
        return Ok(ExitCode::SUCCESS);
    }
    for s in &list {
        println!(
            "  {:<24} {:<20} {:>10}{}  {}",
            s.id,
            s.tool,
            format_size(s.totals.bytes),
            if s.totals.truncated { "+" } else { " " },
            match (s.runnable, s.undo, &s.program) {
                (true, ..) => "run: nfd stores run ".to_owned() + s.id,
                (false, Undo::None, _) => "guide only (cannot be undone)".to_owned(),
                (false, _, None) => "program not on PATH".to_owned(),
                _ => String::new(),
            }
        );
        println!("      {}", s.command_line);
    }
    if list.is_empty() {
        println!("No package manager with a cleanup command was found.");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_one(id: &str, yes: bool, db: Option<PathBuf>) -> anyhow::Result<ExitCode> {
    let list = infos();
    let Some(info) = list.iter().find(|s| s.id == id) else {
        bail!("no cleanup command `{id}` on this machine (see `nfd stores`)");
    };
    if !info.runnable {
        println!(
            "{} is guide-only here. Run it yourself if you are sure:",
            info.id
        );
        println!("  {}", info.command_line);
        return Ok(ExitCode::FAILURE);
    }
    println!("Command: {}", info.command_line);
    println!(
        "Store:   {} ({})",
        format_size(info.totals.bytes),
        info.stores
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !yes {
        eprint!("Run it? The tool downloads what it needs again later. [y/N] ");
        std::io::stderr().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        if !matches!(
            line.trim().to_lowercase().as_str(),
            "y" | "yes" | "c" | "co"
        ) {
            eprintln!("Cancelled.");
            return Ok(ExitCode::FAILURE);
        }
    }
    let started = exec::unix_now();
    let result = stores::run(info, &info.command_line, TIMEOUT, None, 2_000_000)
        .with_context(|| format!("running {}", info.id))?;
    let op = stores::operation(
        info,
        &result,
        exec::new_op_id(started),
        started,
        exec::unix_now(),
    );
    if let Ok(mut h) = crate::diff::open_history(db) {
        let _ = h.record_operation(&op);
    }
    if let Some(dir) = nfd_history::audit::default_dir() {
        let _ = nfd_history::audit::append(&dir, &op);
    }
    let out = result.output.stdout.trim();
    if !out.is_empty() {
        println!("{out}");
    }
    let err = result.output.stderr.trim();
    if !err.is_empty() {
        eprintln!("{err}");
    }
    println!(
        "{} — freed {} ({} → {}). Recorded as operation {} (not restorable).",
        if result.output.success() {
            "Done"
        } else if result.output.timed_out {
            "Timed out"
        } else {
            "Failed"
        },
        format_size(result.freed),
        format_size(result.before),
        format_size(result.after),
        op.id
    );
    Ok(if result.output.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
