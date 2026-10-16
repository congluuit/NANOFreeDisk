//! `nfd diff` — "What changed?" between two saved scans of a folder (PLAN §13 A1),
//! read from the same history database the desktop app writes (`nfd scan --save`
//! adds to it from the command line).

use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::Args;
use nfd_core::units::{format_count, format_size, parse_size};
use nfd_history::{Diff, DiffOptions, History, Snapshot};

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// Folder or drive that was scanned (as given to `nfd scan --save` or the app).
    root: PathBuf,

    /// Compare with the newest saved scan at least this old: 12h, 7d, 2w
    /// [default: the scan just before the newest one].
    #[arg(long, value_name = "AGE", value_parser = parse_age)]
    since: Option<i64>,

    /// Print the diff as JSON.
    #[arg(long)]
    json: bool,

    /// Ignore changes smaller than this: 10MB, 1GB (binary units).
    #[arg(long, default_value = "10MB", value_name = "SIZE", value_parser = parse_size_arg)]
    min_delta: u64,

    /// Rows per list.
    #[arg(long, default_value_t = 20, value_name = "N")]
    top: usize,

    /// History database [default: the desktop app's].
    #[arg(long, value_name = "FILE")]
    db: Option<PathBuf>,

    /// List the saved scans of the folder instead of comparing.
    #[arg(long)]
    list: bool,
}

fn parse_size_arg(s: &str) -> Result<u64, String> {
    parse_size(s).map_err(|e| e.to_string())
}

/// `12h`, `7d`, `2w` → seconds.
pub fn parse_age(s: &str) -> Result<i64, String> {
    let s = s.trim();
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: i64 = num
        .parse()
        .map_err(|_| format!("invalid age {s:?} (use 12h, 7d, 2w)"))?;
    let unit = match unit.trim().to_ascii_lowercase().as_str() {
        "h" => 3_600,
        "" | "d" => 86_400,
        "w" => 7 * 86_400,
        other => return Err(format!("unknown unit {other:?} (use h, d or w)")),
    };
    Ok(n * unit)
}

/// Open the history named by `--db` or the desktop app's default.
pub fn open_history(db: Option<PathBuf>) -> anyhow::Result<History> {
    let path = db
        .or_else(nfd_history::default_path)
        .context("cannot locate the history database; pass --db")?;
    History::open(&path).with_context(|| format!("opening {}", path.display()))
}

/// The snapshot to compare the newest one with.
fn pick_older(snaps: &[Snapshot], since: Option<i64>) -> Option<&Snapshot> {
    let newest = snaps.first()?;
    match since {
        None => snaps.get(1),
        Some(age) => snaps
            .iter()
            .skip(1)
            .find(|s| newest.started_at - s.started_at >= age),
    }
}

pub fn run(args: DiffArgs) -> anyhow::Result<ExitCode> {
    // the same normalization the scanner applies to its root (`/` → `\` on Windows…)
    let root = std::path::absolute(&args.root)
        .unwrap_or_else(|_| args.root.clone())
        .to_string_lossy()
        .into_owned();
    let history = open_history(args.db.clone())?;
    let snaps = history.snapshots(&root)?;
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    if args.list {
        if args.json {
            serde_json::to_writer_pretty(&mut out, &snaps)?;
            writeln!(out)?;
        } else {
            for s in &snaps {
                writeln!(
                    out,
                    "{:>6}  {}  {:>10}  {:>12} files  free {}",
                    s.id,
                    date(s.started_at),
                    format_size(s.on_disk),
                    format_count(s.files),
                    if s.volume_total > 0 {
                        format_size(s.volume_free)
                    } else {
                        "?".into()
                    }
                )?;
            }
        }
        out.flush()?;
        return Ok(ExitCode::SUCCESS);
    }
    let Some(newest) = snaps.first() else {
        bail!(
            "no saved scan of {root}; run `nfd scan {root} --save` (or scan it in the app) first"
        );
    };
    let Some(older) = pick_older(&snaps, args.since) else {
        bail!(
            "only one saved scan of {root} matches{}; scan it again later",
            if args.since.is_some() { " --since" } else { "" }
        );
    };
    let options = DiffOptions {
        min_delta: args.min_delta,
        limit: args.top,
    };
    let diff = history.diff(older.id, newest.id, &options)?;
    if args.json {
        serde_json::to_writer_pretty(&mut out, &diff)?;
        writeln!(out)?;
    } else {
        print_diff(&diff, &mut out)?;
    }
    match out.flush() {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e).context("writing output"),
        _ => Ok(ExitCode::SUCCESS),
    }
}

fn signed(delta: i64) -> String {
    let size = format_size(delta.unsigned_abs());
    match delta.signum() {
        1 => format!("+{size}"),
        -1 => format!("-{size}"),
        _ => size,
    }
}

/// `YYYY-MM-DD HH:MM` (UTC) without a date library.
fn date(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        secs / 3_600,
        secs % 3_600 / 60
    )
}

fn print_diff(d: &Diff, out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "{}", d.newer.root)?;
    writeln!(
        out,
        "  {}  →  {}  ({} days)",
        date(d.older.started_at),
        date(d.newer.started_at),
        (d.newer.started_at - d.older.started_at) / 86_400
    )?;
    write!(
        out,
        "  total {}  ·  files {:+}",
        signed(d.on_disk_delta),
        d.files_delta
    )?;
    if let Some(free) = d.free_delta {
        write!(out, "  ·  free space on the drive {}", signed(free))?;
    }
    writeln!(out)?;
    let dirs = [("Grew", &d.grew), ("Shrank", &d.shrank)];
    for (title, rows) in dirs {
        if rows.is_empty() {
            continue;
        }
        writeln!(out, "\n{title}:")?;
        for c in rows {
            writeln!(out, "  {:>12}  {}", signed(c.delta), c.path)?;
        }
    }
    let files = [
        ("Large files added or grown", &d.files_up),
        ("Large files removed or shrunk", &d.files_down),
    ];
    for (title, rows) in files {
        if rows.is_empty() {
            continue;
        }
        writeln!(out, "\n{title}:")?;
        for f in rows {
            let tag = if f.before == 0 {
                " (new)"
            } else if f.after == 0 {
                " (gone)"
            } else {
                ""
            };
            writeln!(out, "  {:>12}  {}{tag}", signed(f.delta), f.path)?;
        }
    }
    if d.grew.is_empty() && d.shrank.is_empty() && d.files_up.is_empty() && d.files_down.is_empty()
    {
        writeln!(
            out,
            "\nNothing changed by more than the --min-delta threshold."
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(parse_age("12h"), Ok(12 * 3_600));
        assert_eq!(parse_age("7d"), Ok(7 * 86_400));
        assert_eq!(parse_age("7"), Ok(7 * 86_400));
        assert_eq!(parse_age("2W"), Ok(14 * 86_400));
        assert!(parse_age("x").is_err());
        assert!(parse_age("3y").is_err());
    }

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01 00:00 UTC");
        assert_eq!(date(1_792_140_000), "2026-10-16 08:40 UTC");
        assert_eq!(date(951_782_400), "2000-02-29 00:00 UTC");
    }

    fn snap(id: i64, at: i64) -> Snapshot {
        Snapshot {
            id,
            root: "r".into(),
            volume: String::new(),
            started_at: at,
            elapsed_ms: 0,
            files: 0,
            dirs: 0,
            on_disk: 0,
            volume_total: 0,
            volume_free: 0,
            issues: 0,
            dir_floor: 0,
            file_floor: 0,
        }
    }

    #[test]
    fn older_snapshot_choice() {
        let day = 86_400;
        let snaps = [
            snap(4, 10 * day),
            snap(3, 9 * day),
            snap(2, 5 * day),
            snap(1, 0),
        ];
        assert_eq!(pick_older(&snaps, None).map(|s| s.id), Some(3));
        assert_eq!(pick_older(&snaps, Some(2 * day)).map(|s| s.id), Some(2));
        assert_eq!(pick_older(&snaps, Some(7 * day)).map(|s| s.id), Some(1));
        assert_eq!(pick_older(&snaps, Some(30 * day)).map(|s| s.id), None);
        assert_eq!(pick_older(&snaps[..1], None).map(|s| s.id), None);
    }
}
