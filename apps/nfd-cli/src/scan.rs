//! `nfd scan` — run the scanner and print a table, JSON report or CSV export.

use std::io::{self, BufWriter, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Context;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use nfd_core::model::EntryKind;
use nfd_core::scanner::report::{self, Report};
use nfd_core::scanner::{IssueKind, ScanEvent, ScanMode, ScanOptions, ScanStats, ScanTree};
use nfd_core::units::{format_count, format_size, parse_size};

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Directory or drive to scan (e.g. C:\ or %LOCALAPPDATA%).
    path: PathBuf,

    /// Rows per list: largest children, largest files, extensions.
    #[arg(long, default_value_t = 20, value_name = "N")]
    top: usize,

    /// Print the JSON report (schema `nanofreedisk.scan.v1`, see docs/schema/).
    #[arg(long, conflicts_with = "csv")]
    json: bool,

    /// Print every file and folder as CSV: path,kind,logical,on_disk,files,mtime,attrs,hardlink_dup
    /// (ignores --top; use --min-size to shorten it).
    #[arg(long)]
    csv: bool,

    /// Hide rows smaller than this on disk: bytes or 512K, 10MB, 1.5GiB (units are binary).
    #[arg(long, default_value = "0", value_name = "SIZE", value_parser = parse_size_arg)]
    min_size: u64,

    /// Worker threads [default: number of logical CPUs].
    #[arg(long, value_name = "N")]
    threads: Option<NonZeroUsize>,

    /// Read the NTFS MFT directly (admin). Not implemented yet: falls back to the standard scan.
    #[arg(long)]
    fast: bool,

    /// Open every file to get exact hard-link counts (slower on Windows). Totals are
    /// de-duplicated either way.
    #[arg(long)]
    exact_links: bool,

    /// Save a snapshot to the scan history (the desktop app's, or --db) for `nfd diff`.
    #[arg(long)]
    save: bool,

    /// History database used by --save [default: the desktop app's].
    #[arg(long, value_name = "FILE", requires = "save")]
    db: Option<PathBuf>,
}

fn parse_size_arg(s: &str) -> Result<u64, String> {
    parse_size(s).map_err(|e| e.to_string())
}

pub fn run(args: ScanArgs) -> anyhow::Result<ExitCode> {
    let tree = scan_with_progress(&args)?;
    if args.save {
        save(&tree, args.db.clone())?;
    }
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let written = if args.json {
        serde_json::to_writer_pretty(&mut out, &Report::new(&tree, args.top, args.min_size))
            .map_err(io::Error::from)
            .and_then(|()| writeln!(out))
    } else if args.csv {
        report::write_csv(&tree, args.min_size, &mut out)
    } else {
        print_tables(&tree, args.top, args.min_size, &mut out)
    };
    match written.and_then(|()| out.flush()) {
        // `nfd scan … | head` closing the pipe early is not an error.
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e).context("writing output"),
        _ => Ok(ExitCode::SUCCESS),
    }
}

/// Store the scan in the history, with the free space of its volume.
fn save(tree: &ScanTree, db: Option<PathBuf>) -> anyhow::Result<()> {
    let root = tree.root.to_string_lossy().to_lowercase();
    let volume = nfd_platform::volumes::list()
        .into_iter()
        .filter(|v| root.starts_with(&v.root.to_string_lossy().to_lowercase()))
        .max_by_key(|v| v.root.as_os_str().len());
    let (name, state) = volume.map_or_else(
        || (String::new(), nfd_history::VolumeState::default()),
        |v| {
            (
                v.root.to_string_lossy().into_owned(),
                nfd_history::VolumeState {
                    total: v.total,
                    free: v.free,
                },
            )
        },
    );
    let mut history = crate::diff::open_history(db)?;
    let id = history
        .record(tree, &name, state, &nfd_history::RecordOptions::default())
        .context("saving the scan to the history")?;
    eprintln!("saved scan #{id} to the history (compare later with `nfd diff`)");
    Ok(())
}

fn scan_with_progress(args: &ScanArgs) -> anyhow::Result<ScanTree> {
    let options = ScanOptions {
        mode: if args.fast {
            ScanMode::Fast
        } else {
            ScanMode::Standard
        },
        threads: args.threads,
        exact_links: args.exact_links,
        ..ScanOptions::default()
    };
    let path = args.path.clone();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("nfd-scan".into())
        .spawn(move || nfd_core::scanner::scan(&path, &options, Some(&tx)))
        .context("starting scan thread")?;

    // Draws on stderr only when it is a terminal, so redirected output stays clean.
    let bar = ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template("{spinner} scanning {msg}  [{elapsed}]")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    for event in rx {
        match event {
            ScanEvent::Progress(stats) => bar.set_message(progress_line(&stats)),
            ScanEvent::Done { .. } => break,
        }
    }
    bar.finish_and_clear();

    let tree = worker
        .join()
        .map_err(|_| anyhow::anyhow!("scan thread panicked"))??;
    Ok(tree)
}

fn progress_line(s: &ScanStats) -> String {
    let mut line = format!(
        "{} files · {} folders · {}",
        format_count(s.files),
        format_count(s.dirs),
        format_size(s.logical)
    );
    if s.errors > 0 {
        line.push_str(&format!(" · {} unreadable", format_count(s.errors)));
    }
    line
}

fn print_tables(
    tree: &ScanTree,
    top: usize,
    min_size: u64,
    out: &mut impl Write,
) -> io::Result<()> {
    let s = &tree.stats;
    let root_disk = tree.entries.first().map_or(0, |e| e.on_disk);
    writeln!(out, "Scanned {}", tree.root.display())?;
    writeln!(
        out,
        "  {} files · {} folders · {} ({} on disk) · {:.1} s",
        format_count(s.files),
        format_count(s.dirs),
        format_size(s.logical),
        format_size(s.on_disk),
        tree.elapsed.as_secs_f64()
    )?;
    let links = [
        (s.junctions, "junction"),
        (s.symlinks, "symlink"),
        (s.mount_points, "mount point"),
    ];
    let links: Vec<String> = links
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| {
            format!(
                "{} {what}{}",
                format_count(*n),
                if *n == 1 { "" } else { "s" }
            )
        })
        .collect();
    if !links.is_empty() {
        writeln!(out, "  not followed: {}", links.join(", "))?;
    }
    if s.hardlink_dups > 0 {
        writeln!(
            out,
            "  {} duplicate name(s) for already-counted data (hard links), not counted again",
            format_count(s.hardlink_dups)
        )?;
    }
    if s.approximate > 0 {
        writeln!(
            out,
            "  {} item(s) could not be opened; their on-disk size is estimated",
            format_count(s.approximate)
        )?;
    }
    if let Some(reason) = &tree.fallback {
        writeln!(
            out,
            "  fast mode unavailable: {reason}; used the standard scan"
        )?;
    }
    if s.errors > 0 {
        let denied = tree
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::AccessDenied)
            .count();
        writeln!(
            out,
            "  {} path(s) could not be read ({} access denied); sizes below them are missing — \
             run elevated or see --json",
            format_count(s.errors),
            format_count(denied as u64)
        )?;
    }

    let children = tree.top_children(top, min_size);
    if !children.is_empty() {
        writeln!(out, "\nLargest items")?;
        writeln!(
            out,
            "  {:>10}  {:>10}  {:>11}  {:>4}  NAME",
            "ON DISK", "SIZE", "FILES", "%"
        )?;
        for id in children {
            let e = tree.entry(id);
            let (logical, on_disk) = e.counted();
            let suffix = match e.kind {
                EntryKind::Dir => std::path::MAIN_SEPARATOR_STR,
                EntryKind::Junction | EntryKind::Symlink | EntryKind::MountPoint => " ->",
                _ => "",
            };
            writeln!(
                out,
                "  {:>10}  {:>10}  {:>11}  {:>4}  {}{suffix}",
                format_size(on_disk),
                format_size(logical),
                format_count(tree.file_counts[id.index()]),
                percent(on_disk, root_disk),
                e.name
            )?;
        }
    }

    let files = tree.top_files(top, min_size);
    if !files.is_empty() {
        writeln!(out, "\nLargest files")?;
        writeln!(
            out,
            "  {:>10}  {:>10}  {:<10}  PATH",
            "ON DISK", "SIZE", "MODIFIED"
        )?;
        for id in files {
            let e = tree.entry(id);
            writeln!(
                out,
                "  {:>10}  {:>10}  {:<10}  {}",
                format_size(e.on_disk),
                format_size(e.logical),
                date(e.mtime),
                relative(&tree.path(id), &tree.root).display()
            )?;
        }
    }

    let mut exts = tree.extensions();
    exts.retain(|x| x.on_disk >= min_size);
    if !exts.is_empty() {
        writeln!(out, "\nBy extension")?;
        writeln!(
            out,
            "  {:>10}  {:>10}  {:>11}  {:>4}  EXT",
            "ON DISK", "SIZE", "FILES", "%"
        )?;
        for x in exts.iter().take(top) {
            writeln!(
                out,
                "  {:>10}  {:>10}  {:>11}  {:>4}  {}",
                format_size(x.on_disk),
                format_size(x.logical),
                format_count(x.files),
                percent(x.on_disk, root_disk),
                if x.ext.is_empty() { "(none)" } else { &x.ext }
            )?;
        }
    }
    Ok(())
}

fn percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "-".into();
    }
    format!("{}%", (part as f64 * 100.0 / whole as f64).round() as u64)
}

fn relative<'a>(path: &'a Path, root: &Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

/// Unix seconds → `YYYY-MM-DD` (UTC), proleptic Gregorian.
fn date(unix: i64) -> String {
    // Howard Hinnant's civil_from_days.
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
        assert_eq!(date(1_790_000_000), "2026-09-21");
        assert_eq!(date(-86_400), "1969-12-31");
    }

    #[test]
    fn percents() {
        assert_eq!(percent(1, 0), "-");
        assert_eq!(percent(1, 3), "33%");
        assert_eq!(percent(5, 5), "100%");
    }

    #[test]
    fn min_size_arg() {
        assert_eq!(parse_size_arg("10MB"), Ok(10 << 20));
        assert!(parse_size_arg("ten").is_err());
    }
}
