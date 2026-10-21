//! `nfd programs` (PLAN §13.2 E1, `docs/UNINSTALLER.md` §2.6): installed programs,
//! the vendors' uninstallers, the leftovers they leave and software junk.
//!
//! * `list` — the programs "Apps & features" shows (read-only).
//! * `junk` — logs, caches, crash dumps and old updates of installed programs;
//!   dry run unless `--apply` (quarantine).
//! * `uninstall <program>` — runs the vendor's uninstaller, waits for it, then lists
//!   the leftovers; `--remove-leftovers` removes the pre-selected ones.
//! * `leftovers <program>` — the leftovers of a program whose uninstaller is gone
//!   (or, with `--forced`, of one still installed: nothing pre-selected).
//!
//! All-users leftovers (Program Files, ProgramData, `HKEY_LOCAL_MACHINE`) are only
//! removed from an elevated prompt; this user's go to the quarantine and `.reg`
//! backups. Everything lands in the Activity log (`nfd quarantine list`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use nfd_apps::inventory::Program;
use nfd_apps::junk::{self, JunkCtx, JunkReport, JunkRoots};
use nfd_apps::leftovers::{self, Ctx, Kind, Mode, Others, Report, Roots};
use nfd_apps::{Scope, Source, elevated, remove, runner};
use nfd_core::RiskLevel;
use nfd_core::exec::{self, ExecOptions, ItemStatus};
use nfd_core::safety::{LiveProbe, SafetyGate};
use nfd_core::units::format_size;
use nfd_platform::Platform;
use serde_json::json;

/// JSON schema id of `nfd programs --json`.
pub const SCHEMA: &str = "nanofreedisk.programs.v1";

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub struct ProgramsArgs {
    #[command(subcommand)]
    pub action: Option<ProgramsAction>,
    #[command(flatten)]
    pub list: ListArgs,
}

#[derive(Subcommand, Debug)]
pub enum ProgramsAction {
    /// List the installed programs (default).
    List(ListArgs),
    /// Software junk of the installed programs (dry run unless --apply).
    Junk(JunkArgs),
    /// Run a program's own uninstaller, wait for it, then list its leftovers.
    Uninstall(UninstallArgs),
    /// The leftovers of a program whose uninstaller is gone (or, --forced, of one
    /// still installed).
    Leftovers(LeftoverArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct ListArgs {
    /// Include Store (MSIX) apps.
    #[arg(long)]
    pub store: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct JunkArgs {
    /// Also clean 🟡 Caution items (logs outside app caches, Temp folders, old versions).
    #[arg(long)]
    pub caution: bool,
    /// Clean for real (quarantine, restorable for 7 days).
    #[arg(long)]
    pub apply: bool,
    #[arg(long, short = 'y')]
    pub yes: bool,
    #[arg(long)]
    pub json: bool,
    /// Undo journal / history database [default: the desktop app's].
    #[arg(long)]
    pub db: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct UninstallArgs {
    /// Program id (`nfd programs --json`) or exact display name.
    pub program: String,
    /// Use the vendor's silent uninstall command when it registered one.
    #[arg(long)]
    pub quiet: bool,
    /// Remove the pre-selected leftovers afterwards.
    #[arg(long)]
    pub remove_leftovers: bool,
    #[arg(long, short = 'y')]
    pub yes: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub db: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct LeftoverArgs {
    /// Program id (`nfd programs --json`) or exact display name.
    pub program: String,
    /// The program still looks installed: list its leftovers anyway (nothing
    /// pre-selected, its entry never offered).
    #[arg(long)]
    pub forced: bool,
    /// Remove the pre-selected leftovers.
    #[arg(long)]
    pub apply: bool,
    #[arg(long, short = 'y')]
    pub yes: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub db: Option<PathBuf>,
}

fn resolver() -> impl Fn(&str) -> Option<String> {
    move |v| nfd_platform::Native.resolver()(v)
}

fn roots() -> Roots {
    Roots::detect(&resolver(), &|k| std::env::var(k).ok())
}

fn own_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
}

/// `nfd programs`.
pub fn run(args: ProgramsArgs) -> anyhow::Result<ExitCode> {
    match args.action {
        None => list(&args.list),
        Some(ProgramsAction::List(l)) => list(&l),
        Some(ProgramsAction::Junk(j)) => junk_cmd(&j),
        Some(ProgramsAction::Uninstall(u)) => uninstall(&u),
        Some(ProgramsAction::Leftovers(l)) => leftovers_cmd(&l),
    }
}

fn list(args: &ListArgs) -> anyhow::Result<ExitCode> {
    let programs = runner::list_programs(own_dir(), args.store, 200_000);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": SCHEMA, "programs": programs }))?
        );
        return Ok(ExitCode::SUCCESS);
    }
    for p in &programs {
        let mut flags: Vec<&str> = Vec::new();
        if p.scope == Scope::Machine {
            flags.push("all users");
        }
        if p.source == Source::Store {
            flags.push("Store");
        }
        flags.extend(p.caution.as_deref());
        if p.broken {
            flags.push("uninstaller missing");
        }
        if let Some(b) = p.blocked.as_deref() {
            flags.push(b);
        }
        println!(
            "{:>10}  {:<48} {:<28} {}",
            format_size(p.size),
            truncate(&p.name, 48),
            truncate(p.publisher.as_deref().unwrap_or(""), 28),
            if flags.is_empty() {
                String::new()
            } else {
                format!("[{}]", flags.join(", "))
            }
        );
    }
    println!(
        "\n{} program(s), {} in total. Uninstall one with `nfd programs uninstall \"<name>\"`.",
        programs.len(),
        format_size(programs.iter().map(|p| p.size).sum())
    );
    Ok(ExitCode::SUCCESS)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

fn find_program(query: &str) -> anyhow::Result<Program> {
    let programs = runner::list_programs(own_dir(), true, 0);
    programs
        .iter()
        .find(|p| p.id == query)
        .or_else(|| programs.iter().find(|p| p.name.eq_ignore_ascii_case(query)))
        .cloned()
        .with_context(|| {
            format!("no installed program `{query}` (see `nfd programs --json` for ids)")
        })
}

fn junk_cmd(args: &JunkArgs) -> anyhow::Result<ExitCode> {
    let programs = runner::registry_list(own_dir());
    let packs = nfd_rules::builtin_packs()?;
    let resolve = resolver();
    let matcher = nfd_rules::matcher::Matcher::new(&packs, &resolve, std::env::consts::OS);
    let covered = junk::covered_by(&matcher);
    let running = runner::running_images();
    let gate = SafetyGate::new(&resolve);
    let ctx = JunkCtx {
        gate: &gate,
        covered: &covered,
        running: &running,
        max_entries: 2_000_000,
        depth: 5,
        cancel: None,
    };
    let report = junk::scan(&programs, &JunkRoots::detect(&resolve, &[]), &ctx);
    let picked: Vec<PathBuf> = report
        .groups
        .iter()
        .flat_map(|g| g.items.iter())
        .filter(|i| {
            i.blocked.is_none()
                && (i.level == RiskLevel::Safe || (args.caution && i.level == RiskLevel::Caution))
        })
        .map(|i| i.path.clone())
        .collect();
    let plan = junk::plan(&report, &picked);
    if args.json && !args.apply {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({ "schema": SCHEMA, "junk": report, "dryRun": true, "plan": plan })
            )?
        );
        return Ok(ExitCode::SUCCESS);
    }
    if !args.json {
        print_junk(&report);
    }
    if plan.items.is_empty() {
        println!("Nothing to clean.");
        return Ok(ExitCode::SUCCESS);
    }
    if !args.apply {
        println!(
            "\nDry run: would move {} in {} item(s) to the quarantine. Add --apply to do it.",
            format_size(plan.selected_bytes()),
            plan.items.len()
        );
        return Ok(ExitCode::SUCCESS);
    }
    if !crate::quarantine::confirm(&plan, args.yes)? {
        return Ok(ExitCode::FAILURE);
    }
    let (op, journal_ok) =
        crate::quarantine::execute(&plan, "software-junk", args.db.clone(), args.json);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": SCHEMA, "operation": op }))?
        );
    } else {
        crate::quarantine::print_outcome(&op, journal_ok);
    }
    Ok(if op.count(ItemStatus::Failed) > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn print_junk(report: &JunkReport) {
    for g in &report.groups {
        println!(
            "\n== {} ({}){}",
            g.name,
            format_size(g.bytes),
            if g.running {
                format!(" — running: close {} first", g.requires_closed.join(", "))
            } else {
                String::new()
            }
        );
        for i in &g.items {
            println!(
                "   {} {:<9} {:<16} {:>10}  {}{}",
                if i.selected { "x" } else { " " },
                level(i.level),
                i.what,
                format_size(i.bytes),
                i.path.display(),
                i.blocked
                    .as_deref()
                    .map(|b| format!(" (not offered: {b})"))
                    .unwrap_or_default()
            );
        }
    }
    println!(
        "\n{} pre-selected (x, 🟢) of {} found.",
        format_size(report.selected_bytes),
        format_size(report.total_bytes)
    );
}

fn level(l: RiskLevel) -> &'static str {
    match l {
        RiskLevel::Safe => "[safe]",
        RiskLevel::Caution => "[caution]",
        _ => "[advanced]",
    }
}

fn uninstall(args: &UninstallArgs) -> anyhow::Result<ExitCode> {
    let program = find_program(&args.program)?;
    if let Some(b) = &program.blocked {
        bail!("{} cannot be removed from here ({b})", program.name);
    }
    let snap = leftovers::snapshot(&program, &roots(), &runner::system_root(), exec::unix_now());
    let mut mode = Mode::AfterUninstall;
    if program.broken {
        eprintln!(
            "The uninstaller of {} is gone: looking for its leftovers.",
            program.name
        );
        mode = Mode::Broken;
    } else if let Some(pkg) = &program.package {
        eprintln!("Removing the Store app {}…", program.name);
        runner::remove_store_package(&pkg.full_name)?;
    } else {
        let cmd = args
            .quiet
            .then_some(program.quiet_uninstall.as_deref())
            .flatten()
            .and_then(|q| {
                nfd_apps::command::parse(
                    q,
                    &|k| std::env::var(k).ok(),
                    &|p: &Path| p.exists(),
                    &runner::system_root(),
                )
            })
            .or_else(|| program.uninstall.clone())
            .context("no uninstaller registered")?;
        eprintln!("Running {}", cmd.line());
        eprintln!(
            "Follow the uninstaller's window; this waits until it (and what it starts) has finished."
        );
        let started = runner::start_uninstaller(&cmd, 0)?;
        let stop = AtomicBool::new(false);
        let waited = runner::wait_tree(
            started.pid,
            started.launched,
            &runner::process_snapshot,
            &stop,
            Duration::from_millis(500),
            Duration::from_secs(4 * 3600),
            &mut |_| {},
        );
        if waited != runner::Waited::Done {
            eprintln!("Stopped waiting ({waited:?}).");
        }
        let still = runner::registry_list(own_dir())
            .iter()
            .any(|p| p.id == program.id)
            && program
                .uninstall
                .as_ref()
                .is_none_or(|c| c.file_name() == "msiexec.exe" || c.exe.exists());
        if still {
            eprintln!(
                "{} is still installed (the uninstaller was cancelled?). Nothing else to do.",
                program.name
            );
            return Ok(ExitCode::FAILURE);
        }
        eprintln!("{} was uninstalled. Looking for leftovers…", program.name);
    }
    let report = scan(&snap, mode);
    finish(
        &report,
        args.remove_leftovers,
        args.yes,
        args.json,
        args.db.clone(),
    )
}

fn leftovers_cmd(args: &LeftoverArgs) -> anyhow::Result<ExitCode> {
    let program = find_program(&args.program)?;
    let mode = if args.forced {
        Mode::Forced
    } else if program.broken {
        Mode::Broken
    } else {
        bail!(
            "{} is still installed: uninstall it with `nfd programs uninstall`, or add --forced to list what it would leave",
            program.name
        );
    };
    let snap = leftovers::snapshot(&program, &roots(), &runner::system_root(), exec::unix_now());
    let report = scan(&snap, mode);
    finish(&report, args.apply, args.yes, args.json, args.db.clone())
}

fn scan(snap: &leftovers::Snapshot, mode: Mode) -> Report {
    let list = runner::registry_list(own_dir());
    let roots = roots();
    let system_root = runner::system_root();
    let others = Others::new(&list, &snap.program.id, &roots, &system_root);
    let running = runner::running_images();
    let resolve = resolver();
    let env = move |k: &str| std::env::var(k).ok().or_else(|| resolve(&k.to_uppercase()));
    let gate = SafetyGate::new(&resolver());
    let ctx = Ctx {
        reg: &nfd_apps::registry::LiveRegistry,
        roots: &roots,
        gate: &gate,
        others: &others,
        running: &running,
        max_entries: 2_000_000,
        large: leftovers::LARGE_LEFTOVER,
        exists: &|p: &Path| p.exists(),
        env: &env,
        system_root,
    };
    leftovers::scan(snap, &ctx, mode)
}

fn print_leftovers(report: &Report) {
    println!("\nLeftovers of {}:", report.program.name);
    for i in &report.items {
        let what = match i.kind {
            Kind::Folder => "folder",
            Kind::File => "file",
            Kind::Shortcut => "shortcut",
            Kind::RegistryKey => "registry key",
            Kind::RegistryValue => "registry value",
            Kind::Service => "service",
        };
        let path = match &i.value {
            Some(v) => format!("{} → {v}", i.path),
            None => i.path.clone(),
        };
        println!(
            "   {} {:<9} {:<15} {:>10}  {}{}{}",
            if i.selected { "x" } else { " " },
            level(i.level),
            what,
            if i.bytes > 0 {
                format_size(i.bytes)
            } else {
                String::new()
            },
            path,
            if i.scope == Scope::Machine {
                "  [all users]"
            } else {
                ""
            },
            i.blocked
                .as_deref()
                .map(|b| format!(" (not offered: {b})"))
                .or_else(|| i.also.as_deref().map(|a| format!(" (also fits {a})")))
                .or_else(|| i.large.then(|| " (large: look inside first)".to_owned()))
                .unwrap_or_default()
        );
        if i.kind == Kind::Service {
            if let Some(name) = &i.detail {
                println!("        remove it from an administrator prompt: sc delete \"{name}\"");
            }
        }
    }
    println!(
        "\n{} pre-selected (x, 🟢) of {} found.",
        format_size(report.selected_bytes),
        format_size(report.total_bytes)
    );
}

fn finish(
    report: &Report,
    apply: bool,
    yes: bool,
    json: bool,
    db: Option<PathBuf>,
) -> anyhow::Result<ExitCode> {
    if !json {
        print_leftovers(report);
    }
    let picked = report.preselected();
    if !apply || picked.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({ "schema": SCHEMA, "leftovers": report, "dryRun": true })
                )?
            );
        } else if !picked.is_empty() {
            println!(
                "Remove the pre-selected ones with --apply (this user's to the quarantine and .reg backups)."
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    let local = nfd_history::default_path()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .context("no data folder")?;
    let now = exec::unix_now();
    let op_id = exec::new_op_id(now);
    let prepared = remove::prepare(
        report,
        &picked,
        &local.join("registry-backup").join(&op_id),
        &op_id,
    );
    if !yes && !json {
        let answer = crate::quarantine::ask(&format!(
            "Remove {} leftover(s) of {} ({})? Files go to the quarantine, registry keys to .reg backups. [y/N] ",
            picked.len(),
            report.program.name,
            format_size(prepared.bytes())
        ))?;
        if !answer.eq_ignore_ascii_case("y") {
            return Ok(ExitCode::FAILURE);
        }
    }
    let resolve = resolver();
    let gate = SafetyGate::new(&resolve);
    let opts = ExecOptions {
        op_id: op_id.clone(),
        kind: "uninstall".into(),
        owner: 0,
        roots: crate::quarantine::roots(),
        now,
    };
    let mut op = exec::execute(&prepared.plan, &gate, &LiveProbe, &opts, &mut |_, _, _| {
        true
    });
    let first = op.items.len();
    op.items
        .extend(remove::run_registry(&prepared.registry, first, false));
    if !prepared.machine.is_empty() {
        let base = op.items.len();
        if nfd_platform::registry::is_elevated() {
            let resp = elevated::execute(&prepared.machine, &gate, &roots());
            for mut item in resp.items {
                item.n += base;
                op.items.push(item);
            }
        } else {
            eprintln!(
                "{} all-users leftover(s) need an administrator prompt: run this command again from one.",
                prepared.machine.files.len() + prepared.machine.registry.len()
            );
        }
    }
    op.finished_at = exec::unix_now();
    let journal_ok = crate::quarantine::record(&op, db);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": SCHEMA, "operation": op }))?
        );
    } else {
        crate::quarantine::print_outcome(&op, journal_ok);
    }
    Ok(if op.count(ItemStatus::Failed) > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}
