//! `nfd dupes` — files with identical contents (Phase 5, `docs/PHASE5_DUPLICATES.md`).

use std::io::{self, BufWriter, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Context;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use nfd_core::plan::CleanPlan;
use nfd_core::safety::SafetyGate;
use nfd_core::scanner::{self, ScanEvent, ScanOptions, ScanTree};
use nfd_core::units::{format_count, format_size, parse_size};
use nfd_dupes::cache::HashCache;
use nfd_dupes::hash::LiveSource;
use nfd_dupes::keep::{KeepStrategy, auto_select};
use nfd_dupes::plan::{Action, Verifier, plan};
use nfd_dupes::{CopyFlags, DupeOptions, DupeReport, Phase, Progress};
use nfd_platform::Platform;

#[derive(Args, Debug)]
pub struct DupesArgs {
    /// Folders or drives to search (several may be given).
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    /// Ignore files smaller than this: bytes or 512K, 10MB, 1.5GiB.
    #[arg(long, default_value = "1MB", value_name = "SIZE", value_parser = parse_size_arg)]
    min_size: u64,

    /// Only these extensions (comma separated, e.g. jpg,png,mp4).
    #[arg(long, value_delimiter = ',', value_name = "EXT")]
    ext: Vec<String>,

    /// Also look inside application folders (node_modules, .git, Steam libraries,
    /// %LOCALAPPDATA%\Programs…); their copies are listed but never picked.
    #[arg(long)]
    include_app_dirs: bool,

    /// After the hashes, compare every group byte by byte.
    #[arg(long)]
    paranoid: bool,

    /// Files hashed at once on an SSD [default: 4]; spinning disks always read one
    /// file at a time.
    #[arg(long, value_name = "N")]
    threads: Option<NonZeroUsize>,

    /// Groups listed in the table.
    #[arg(long, default_value_t = 20, value_name = "N")]
    top: usize,

    /// Print the JSON report (schema `nanofreedisk.dupes.v1`).
    #[arg(long, conflicts_with = "csv")]
    json: bool,

    /// Print every copy as CSV: group,hash,size,wasted,path,mtime,flags.
    #[arg(long)]
    csv: bool,

    /// Do not read or write the hash cache.
    #[arg(long)]
    no_cache: bool,

    /// History database holding the hash cache and the undo journal [default: the
    /// desktop app's].
    #[arg(long, value_name = "FILE")]
    db: Option<PathBuf>,

    /// Which copy stays: auto (outside Downloads/Temp/Desktop, then the oldest),
    /// oldest, newest, shortest, or prefer:<dir>[;<dir>…]. Copies in cloud-sync
    /// folders, git working trees, application folders and read-only files are kept
    /// and never removed by this command.
    #[arg(long, default_value = "auto", value_name = "STRATEGY")]
    keep: KeepStrategy,

    /// What to do with the other copies: list (default), recycle (Recycle Bin) or
    /// quarantine (restorable for 7 days). Nothing happens without --apply.
    #[arg(long, value_enum, default_value_t = DupeAction::List)]
    action: DupeAction,

    /// Carry out --action (after a confirmation unless --yes).
    #[arg(long)]
    apply: bool,

    /// Do not ask for confirmation.
    #[arg(long, requires = "apply")]
    yes: bool,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DupeAction {
    List,
    Recycle,
    Quarantine,
}

fn parse_size_arg(s: &str) -> Result<u64, String> {
    parse_size(s).map_err(|e| e.to_string())
}

pub fn run(args: DupesArgs) -> anyhow::Result<ExitCode> {
    let report = search(&args)?;
    if args.action != DupeAction::List {
        return act(&args, &report);
    }
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let written = if args.json {
        let mut v = serde_json::to_value(&report)?;
        v["schema"] = nfd_dupes::SCHEMA.into();
        v["version"] = env!("CARGO_PKG_VERSION").into();
        serde_json::to_writer_pretty(&mut out, &v)
            .map_err(io::Error::from)
            .and_then(|()| writeln!(out))
    } else if args.csv {
        write_csv(&report, &mut out)
    } else {
        print_report(&report, args.top, &mut out)
    };
    match written.and_then(|()| out.flush()) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e).context("writing output"),
        _ => Ok(ExitCode::SUCCESS),
    }
}

/// Scan every path, then run the duplicate pipeline (with the hash cache).
pub fn search(args: &DupesArgs) -> anyhow::Result<DupeReport> {
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    let gate = SafetyGate::new(&resolve);
    let mut trees = Vec::new();
    for p in &args.paths {
        trees.push(scan(p)?);
    }
    let opts = DupeOptions {
        min_size: args.min_size,
        include_app_dirs: args.include_app_dirs,
        paranoid: args.paranoid,
        threads: args.threads,
        extensions: args.ext.clone(),
        app_dir_paths: nfd_dupes::collect::default_app_dirs(&resolve),
        cancel: None,
    };
    let mut history = if args.no_cache {
        None
    } else {
        crate::diff::open_history(args.db.clone()).ok()
    };
    let bar = ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template("{spinner} {msg}  [{elapsed}]")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    let refs: Vec<&ScanTree> = trees.iter().collect();
    let report = nfd_dupes::find(
        &refs,
        &gate,
        &opts,
        history.as_mut().map(|h| h as &mut dyn HashCache),
        &LiveSource,
        &|p: &Progress| bar.set_message(progress_line(p)),
    )?;
    bar.finish_and_clear();
    Ok(report)
}

fn progress_line(p: &Progress) -> String {
    let what = match p.phase {
        Phase::Collect => "looking for candidates",
        Phase::Partial => "sampling",
        Phase::Full => "hashing",
        Phase::Compare => "comparing bytes",
    };
    if p.total_bytes > 0 {
        format!(
            "{what} {}/{} files · {} of {}",
            format_count(p.done),
            format_count(p.total),
            format_size(p.bytes),
            format_size(p.total_bytes)
        )
    } else {
        format!(
            "{what} {}/{} files",
            format_count(p.done),
            format_count(p.total)
        )
    }
}

fn scan(path: &Path) -> anyhow::Result<ScanTree> {
    let options = ScanOptions::default();
    let root = path.to_path_buf();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("nfd-scan".into())
        .spawn(move || scanner::scan(&root, &options, Some(&tx)))
        .context("starting scan thread")?;
    let bar = ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template("{spinner} scanning {msg}  [{elapsed}]")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    for event in rx {
        match event {
            ScanEvent::Progress(s) => bar.set_message(format!(
                "{} · {} files",
                path.display(),
                format_count(s.files)
            )),
            ScanEvent::Done { .. } => break,
        }
    }
    bar.finish_and_clear();
    worker
        .join()
        .map_err(|_| anyhow::anyhow!("scan thread panicked"))?
        .with_context(|| format!("scanning {}", path.display()))
}

/// The plan for `--action`, printed (dry run) or carried out (`--apply`).
fn act(args: &DupesArgs, report: &DupeReport) -> anyhow::Result<ExitCode> {
    let action = match args.action {
        DupeAction::Quarantine => Action::Quarantine,
        _ => Action::Recycle,
    };
    let selections = auto_select(&report.groups, &args.keep);
    let plan: CleanPlan = plan(&report.groups, &selections, action)?;
    if !args.json {
        for s in &selections {
            println!("\n#{} keep {}", s.group, s.keep.display());
            for p in &s.remove {
                println!("   remove {}", p.display());
            }
        }
    }
    if plan.items.is_empty() {
        println!("Nothing to remove.");
        return Ok(ExitCode::SUCCESS);
    }
    let target = match action {
        Action::Recycle => "the Recycle Bin",
        Action::Quarantine => "the quarantine",
    };
    if !args.apply {
        if args.json {
            let v = serde_json::json!({
                "schema": nfd_dupes::SCHEMA,
                "report": report,
                "selections": selections,
                "plan": plan,
            });
            println!("{}", serde_json::to_string_pretty(&v)?);
        } else {
            println!(
                "\nDry run: would move {} in {} file(s) to {target}, keeping one copy of {} group(s). Add --apply to do it.",
                format_size(plan.selected_bytes()),
                plan.items.len(),
                selections.len()
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    if !crate::quarantine::confirm(&plan, args.yes)? {
        return Ok(ExitCode::FAILURE);
    }
    // every copy and its keeper are hashed again right before the move
    let mut verifier = Verifier::new(&LiveSource, &report.groups);
    let (op, journal_ok) = crate::quarantine::execute_verified(
        &plan,
        "dupes",
        args.db.clone(),
        args.json,
        &mut |item| verifier.check(item),
    );
    if args.json {
        let v = serde_json::json!({
            "schema": nfd_dupes::SCHEMA,
            "selections": selections,
            "operation": op,
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        crate::quarantine::print_outcome(&op, journal_ok);
    }
    Ok(crate::quarantine::exit_code(&op))
}

/// Short markers for a copy's flags.
pub fn flag_text(f: CopyFlags) -> String {
    let mut out = Vec::new();
    if f.cloud {
        out.push("cloud-sync");
    }
    if f.git {
        out.push("git");
    }
    if f.app_dir {
        out.push("app");
    }
    if f.readonly {
        out.push("read-only");
    }
    out.join(" ")
}

fn date(ts: i64) -> String {
    let (y, m, d, h, mi, _) = nfd_core::exec::civil(ts);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}")
}

fn print_report(r: &DupeReport, top: usize, out: &mut impl Write) -> io::Result<()> {
    for g in r.groups.iter().take(top) {
        writeln!(
            out,
            "\n#{:<4} {} × {} = {} wasted  ({})",
            g.id,
            g.copies.len(),
            format_size(g.size),
            format_size(g.wasted),
            &g.hash[..g.hash.len().min(16)]
        )?;
        for c in &g.copies {
            let flags = flag_text(c.flags);
            writeln!(
                out,
                "   {}  {}{}",
                date(c.mtime),
                c.path.display(),
                if flags.is_empty() {
                    String::new()
                } else {
                    format!("  [{flags}]")
                }
            )?;
            for l in &c.links {
                writeln!(out, "                     = {} (hard link)", l.display())?;
            }
        }
    }
    if r.groups.len() > top {
        writeln!(
            out,
            "\n… {} more group(s) (--top N to list more)",
            r.groups.len() - top
        )?;
    }
    let skipped: Vec<String> = r
        .skipped
        .iter()
        .filter(|(k, _)| !matches!(k, nfd_dupes::SkipReason::Small))
        .map(|(k, v)| format!("{} {}", format_count(*v), k.as_str()))
        .collect();
    writeln!(
        out,
        "\n{} group(s) of identical files, {} wasted. Looked at {} files ({} candidates), read {}, {} from the cache, {:.1} s.",
        format_count(r.groups.len() as u64),
        format_size(r.wasted_bytes),
        format_count(r.scanned_files),
        format_count(r.candidates),
        format_size(r.hashed_bytes),
        format_count(r.cache_hits),
        r.elapsed_ms as f64 / 1000.0
    )?;
    if !skipped.is_empty() {
        writeln!(out, "Left out: {}.", skipped.join(", "))?;
    }
    Ok(())
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// One line per copy (and per hard-linked name).
pub fn write_csv(r: &DupeReport, out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "group,hash,size,wasted,path,mtime,flags")?;
    for g in &r.groups {
        for c in &g.copies {
            for p in std::iter::once(&c.path).chain(&c.links) {
                writeln!(
                    out,
                    "{},{},{},{},{},{},{}",
                    g.id,
                    g.hash,
                    g.size,
                    g.wasted,
                    csv_field(&p.to_string_lossy()),
                    c.mtime,
                    flag_text(c.flags)
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use nfd_dupes::{DupeGroup, FileCopy};

    use super::*;

    fn copy(path: &str, flags: CopyFlags) -> FileCopy {
        FileCopy {
            path: PathBuf::from(path),
            mtime: 1_700_000_000,
            volume: 1,
            file_id: Some(7),
            links: Vec::new(),
            flags,
        }
    }

    #[test]
    fn csv_lists_every_copy_and_quotes_paths() {
        let mut a = copy("D:/a,b.jpg", CopyFlags::default());
        a.links.push(PathBuf::from("D:/link.jpg"));
        let report = DupeReport {
            groups: vec![DupeGroup {
                id: 1,
                hash: "ab".repeat(32),
                size: 10,
                wasted: 10,
                copies: vec![
                    a,
                    copy(
                        "D:/OneDrive/a.jpg",
                        CopyFlags {
                            cloud: true,
                            git: true,
                            ..CopyFlags::default()
                        },
                    ),
                ],
            }],
            ..DupeReport::default()
        };
        let mut out = Vec::new();
        write_csv(&report, &mut out).expect("csv");
        let text = String::from_utf8(out).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4, "{text}");
        assert!(lines[1].contains("\"D:/a,b.jpg\""));
        assert!(lines[2].contains("D:/link.jpg"));
        assert!(lines[3].ends_with("cloud-sync git"));
    }
}
