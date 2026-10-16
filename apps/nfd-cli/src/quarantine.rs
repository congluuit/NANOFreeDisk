//! `nfd clean --apply` and `nfd quarantine list | show | restore | purge`.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use clap::Subcommand;
use indicatif::{ProgressBar, ProgressStyle};
use nfd_core::exec::{self, ExecOptions, ItemStatus, Operation, QuarantineRoots};
use nfd_core::plan::{CleanPlan, DeleteMode};
use nfd_core::safety::{LiveProbe, SafetyGate};
use nfd_core::units::{format_count, format_size};
use nfd_platform::Platform;
use nfd_rules::matcher::MatchReport;

use crate::clean::CleanArgs;

#[derive(Subcommand, Debug)]
pub enum QuarantineAction {
    /// List clean / delete operations, newest first.
    List {
        /// How many operations to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// History database [default: the desktop app's].
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Show the items of one operation.
    Show {
        id: String,
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Put the items of an operation back where they were.
    Restore {
        id: String,
        /// Only these item numbers (see `show`).
        #[arg(long, value_delimiter = ',')]
        item: Vec<u64>,
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Delete quarantined items for good: one operation, or everything older than
    /// --older-than (default 7d).
    Purge {
        /// Operation id (omit to purge by age).
        id: Option<String>,
        /// Age like `7d`, `12h` or `30d`.
        #[arg(long, default_value = "7d")]
        older_than: String,
        #[arg(long)]
        db: Option<PathBuf>,
    },
}

/// The quarantine roots used by the app and the CLI.
pub fn roots() -> QuarantineRoots {
    let app_local = nfd_history::default_path().and_then(|p| p.parent().map(PathBuf::from));
    QuarantineRoots::new(app_local.as_deref())
}

fn parse_age(text: &str) -> anyhow::Result<i64> {
    let text = text.trim();
    let (num, unit) = text.split_at(text.trim_end_matches(char::is_alphabetic).len());
    let n: i64 = num.parse().with_context(|| format!("bad age `{text}`"))?;
    let secs = match unit {
        "d" | "" => n * 86_400,
        "h" => n * 3_600,
        "w" => n * 7 * 86_400,
        _ => anyhow::bail!("bad age unit in `{text}` (use d, h or w)"),
    };
    Ok(secs)
}

fn ask(prompt: &str) -> anyhow::Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

/// Ask before acting on `plan` (unless `yes`; a permanent plan always asks for
/// `DELETE`). `Ok(false)` = the user cancelled.
pub fn confirm(plan: &CleanPlan, yes: bool) -> anyhow::Result<bool> {
    let selected = plan.selected().count();
    let bytes = plan.selected_bytes();
    let permanent = plan.selected().any(|i| i.mode == DeleteMode::Permanent);
    let ok = if permanent {
        let answer = ask(&format!(
            "Permanently delete {} in {selected} item(s)? This cannot be undone. Type DELETE to continue: ",
            format_size(bytes)
        ))?;
        answer == "DELETE"
    } else if yes {
        true
    } else {
        let to_bin = plan.selected().all(|i| i.mode == DeleteMode::RecycleBin);
        let answer = ask(&if to_bin {
            format!(
                "Move {} in {selected} item(s) to the Recycle Bin? [y/N] ",
                format_size(bytes)
            )
        } else {
            format!(
                "Move {} in {selected} item(s) to the quarantine (restorable for {} days)? [y/N] ",
                format_size(bytes),
                exec::QUARANTINE_TTL_DAYS
            )
        })?;
        matches!(answer.to_lowercase().as_str(), "y" | "yes" | "c" | "co")
    };
    if !ok {
        eprintln!("Cancelled.");
    }
    Ok(ok)
}

/// Run a confirmed plan through the SafetyGate (live checks) and the executor, then
/// record it in the undo journal and the audit log. Returns the operation and
/// whether the journal was written.
pub fn execute(
    plan: &CleanPlan,
    kind: &str,
    db: Option<PathBuf>,
    quiet: bool,
) -> (Operation, bool) {
    execute_verified(plan, kind, db, quiet, &mut |_| Ok(()))
}

/// [`execute`] with a last feature-specific check per item (Duplicates).
pub fn execute_verified(
    plan: &CleanPlan,
    kind: &str,
    db: Option<PathBuf>,
    quiet: bool,
    verify: &mut dyn FnMut(&nfd_core::plan::PlanItem) -> Result<(), exec::Refusal>,
) -> (Operation, bool) {
    let platform = nfd_platform::Native;
    let gate = SafetyGate::new(&platform.resolver());
    let roots = roots();
    let mut history = crate::diff::open_history(db).ok();
    if let Some(h) = history.as_mut() {
        let _ = h.purge_expired(&roots, exec::unix_now(), None);
    }
    let now = exec::unix_now();
    let opts = ExecOptions {
        op_id: exec::new_op_id(now),
        kind: kind.into(),
        owner: 0,
        roots,
        now,
    };
    let bar = if quiet {
        ProgressBar::hidden()
    } else {
        ProgressBar::new(plan.items.len() as u64)
    };
    bar.set_style(
        ProgressStyle::with_template("{bar:30} {pos}/{len} {wide_msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    let op = exec::execute_verified(plan, &gate, &LiveProbe, &opts, verify, &mut |done, _, r| {
        bar.set_position(done as u64);
        bar.set_message(r.path.display().to_string());
        true
    });
    bar.finish_and_clear();

    let mut journal_ok = false;
    if let Some(h) = history.as_mut() {
        journal_ok = h.record_operation(&op).is_ok();
    }
    if let Some(dir) = nfd_history::audit::default_dir() {
        let _ = nfd_history::audit::append(&dir, &op);
    }
    (op, journal_ok)
}

/// Problems, totals and the undo hint of an operation.
pub fn print_outcome(op: &Operation, journal_ok: bool) {
    for r in op
        .items
        .iter()
        .filter(|r| !matches!(r.status, ItemStatus::Done | ItemStatus::NotSelected))
    {
        println!(
            "  {:<9} {}{}",
            r.status.as_str(),
            r.path.display(),
            r.reason
                .as_deref()
                .map(|x| format!(" ({x})"))
                .unwrap_or_default()
        );
    }
    println!(
        "Freed {} — {} done, {} partly (files in use), {} skipped, {} failed. Operation {}.",
        format_size(op.freed()),
        format_count(op.count(ItemStatus::Done) as u64),
        op.count(ItemStatus::Partial),
        op.count(ItemStatus::Skipped),
        op.count(ItemStatus::Failed),
        op.id
    );
    if journal_ok {
        println!("Undo with `nfd quarantine restore {}`.", op.id);
    } else {
        println!(
            "warning: the undo journal could not be written; quarantined items are listed in each `manifest.jsonl`."
        );
    }
}

/// Exit code of an operation: failure when an item failed.
pub fn exit_code(op: &Operation) -> ExitCode {
    if op.count(ItemStatus::Failed) == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `nfd clean --apply`.
pub fn apply(args: &CleanArgs, report: &MatchReport, plan: CleanPlan) -> anyhow::Result<ExitCode> {
    if plan.selected().next().is_none() {
        println!("Nothing to clean.");
        return Ok(ExitCode::SUCCESS);
    }
    if !args.json {
        for r in report.rules.iter().filter(|r| !r.running.is_empty()) {
            eprintln!(
                "note: {} is running ({}); files it keeps open are skipped",
                r.app,
                r.running.join(", ")
            );
        }
    }
    if !confirm(&plan, args.yes)? {
        return Ok(ExitCode::FAILURE);
    }
    let (op, journal_ok) = execute(&plan, "clean", args.db.clone(), args.json);
    if args.json {
        let mut value = crate::clean::to_json(report, &plan, false);
        value["operation"] = serde_json::to_value(&op)?;
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        print_outcome(&op, journal_ok);
    }
    Ok(exit_code(&op))
}

pub fn run(action: QuarantineAction) -> anyhow::Result<ExitCode> {
    match action {
        QuarantineAction::List { limit, db, json } => {
            let mut h = crate::diff::open_history(db)?;
            let _ = h.purge_expired(&roots(), exec::unix_now(), None);
            let ops = h.operations(limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&ops)?);
                return Ok(ExitCode::SUCCESS);
            }
            if ops.is_empty() {
                println!("No clean or delete operations yet.");
            }
            for o in ops {
                let (y, m, d, hh, mm, _) = exec::civil(o.started_at);
                println!(
                    "{}  {y:04}-{m:02}-{d:02} {hh:02}:{mm:02}Z  {:<7} {:>10} freed  {:>4} item(s)  {}{}",
                    o.id,
                    o.kind,
                    format_size(o.freed),
                    o.done,
                    o.state,
                    if o.restorable > 0 {
                        format!(" ({} restorable)", o.restorable)
                    } else {
                        String::new()
                    }
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        QuarantineAction::Show { id, db, json } => {
            let h = crate::diff::open_history(db)?;
            let (row, items) = h.operation(&id)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({ "operation": row, "items": items })
                    )?
                );
                return Ok(ExitCode::SUCCESS);
            }
            println!("{} ({}, {})", row.id, row.kind, row.state);
            for i in items {
                println!(
                    "  #{:<4} {:<9} {:<11} {:>10}  {}{}",
                    i.n,
                    i.status,
                    i.mode,
                    format_size(i.freed),
                    i.path,
                    if i.restored { "  [restored]" } else { "" }
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        QuarantineAction::Restore { id, item, db } => {
            let mut h = crate::diff::open_history(db)?;
            let only = (!item.is_empty()).then_some(item.as_slice());
            let results = h.restore_operation(&id, only)?;
            if results.is_empty() {
                println!("Nothing left to restore in {id}.");
            }
            let mut failed = 0;
            for r in &results {
                println!(
                    "  {:<14} {}{}",
                    format!("{:?}", r.status).to_lowercase(),
                    r.path,
                    r.message
                        .as_deref()
                        .map(|m| format!(" — {m}"))
                        .unwrap_or_default()
                );
                failed += usize::from(matches!(
                    r.status,
                    exec::RestoreStatus::Failed | exec::RestoreStatus::Conflicts
                ));
            }
            if let Some(dir) = nfd_history::audit::default_dir() {
                let _ = nfd_history::audit::note(
                    &dir,
                    exec::unix_now(),
                    &format!("{id}\trestore\t{} item(s)", results.len()),
                );
            }
            Ok(if failed == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        QuarantineAction::Purge { id, older_than, db } => {
            let mut h = crate::diff::open_history(db)?;
            let purged = match id {
                Some(id) => {
                    h.purge_operation(&id)?;
                    vec![id]
                }
                None => {
                    let secs = parse_age(&older_than)?;
                    h.purge_expired(&roots(), exec::unix_now(), Some(secs / 86_400))?
                }
            };
            println!("Purged {} operation(s).", purged.len());
            if let Some(dir) = nfd_history::audit::default_dir() {
                for p in &purged {
                    let _ =
                        nfd_history::audit::note(&dir, exec::unix_now(), &format!("{p}\tpurge"));
                }
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(parse_age("7d").expect("7d"), 7 * 86_400);
        assert_eq!(parse_age("12h").expect("12h"), 12 * 3_600);
        assert_eq!(parse_age("2w").expect("2w"), 14 * 86_400);
        assert!(parse_age("x").is_err());
        assert!(parse_age("3y").is_err());
    }
}
