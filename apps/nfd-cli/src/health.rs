//! `nfd health` — drive diagnostics (PLAN §4.6, docs/PHASE7_HEALTH.md): SMART / NVMe
//! verdict per drive and read-only volume checks. Run from an administrator terminal
//! to include SMART thresholds, USB bridges and the volume dirty bit.
//!
//! Exit code: 0 all drives good (or unreadable), 1 a drive needs attention, 2 a
//! drive is bad — so scripts can act on it.

use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use nfd_core::units::format_size;
use nfd_health::{Drive, HealthReport, Options, Status};

#[derive(Args, Debug)]
pub struct HealthArgs {
    /// Print the report as JSON (`nanofreedisk.health.v1`).
    #[arg(long)]
    json: bool,

    /// Only this drive (`N` of `\\.\PhysicalDriveN`).
    #[arg(long, value_name = "N")]
    disk: Option<u32>,

    /// Skip the volume checks.
    #[arg(long)]
    no_volumes: bool,

    /// Also list every SMART attribute / NVMe log field.
    #[arg(long)]
    raw: bool,

    /// Leave serial numbers out (for reports sent to someone else).
    #[arg(long)]
    redact: bool,

    /// History database for the "full in ~N days" forecast [default: the desktop app's].
    #[arg(long, value_name = "FILE")]
    db: Option<PathBuf>,
}

/// Days of history the forecast fits (PLAN §4.6: 30-day trend).
pub const TREND_DAYS: i64 = 30;

/// Fill in the free-space forecast from the scan history (never creates the DB).
pub fn add_forecast(report: &mut HealthReport, db: Option<PathBuf>) {
    let Some(path) = db.or_else(nfd_history::default_path) else {
        return;
    };
    if !path.is_file() {
        return;
    }
    let Ok(history) = nfd_history::History::open(&path) else {
        return;
    };
    let now = report.generated_at;
    for v in &mut report.volumes {
        if let Ok(Some(f)) = history.forecast(&v.root, now, v.free, v.total, TREND_DAYS) {
            v.has_trend = true;
            v.days_to_full = f.days_to_full;
        }
    }
}

pub fn run(args: HealthArgs) -> anyhow::Result<ExitCode> {
    let mut report = nfd_health::collect(Options {
        volumes: !args.no_volumes,
        privileged: true,
    });
    if let Some(n) = args.disk {
        report.drives.retain(|d| d.index == n);
        report.volumes.retain(|v| v.disk == Some(n));
        if report.drives.is_empty() {
            anyhow::bail!("no drive {n} (see `nfd health` for the list)");
        }
    }
    if args.redact {
        report.redact();
    }
    add_forecast(&mut report, args.db);
    let out = io::stdout();
    let mut w = BufWriter::new(out.lock());
    if args.json {
        serde_json::to_writer_pretty(&mut w, &report)?;
        writeln!(w)?;
    } else {
        print_text(&mut w, &report, args.raw)?;
    }
    w.flush()?;
    Ok(ExitCode::from(report.exit_code() as u8))
}

fn status_word(s: Status) -> &'static str {
    match s {
        Status::Good => "Good",
        Status::Caution => "Caution",
        Status::Bad => "Bad",
        Status::Unknown => "Unknown",
    }
}

/// Plain-English reason (the app localizes the same keys).
fn reason_text(d: &Drive, r: &nfd_health::Reason) -> String {
    let v = r.value.unwrap_or(0.0);
    let attr = r.attr.map(|a| {
        let name = nfd_health::ata::name(a).unwrap_or("vendor specific");
        format!("{a:02X} {name}")
    });
    match r.key.as_str() {
        "nvme-spare-low" => format!("available spare {v:.0} % is below the threshold"),
        "nvme-reliability" => "the drive reports degraded reliability".into(),
        "nvme-read-only" => "the drive switched to read-only mode".into(),
        "nvme-backup" => "the volatile memory backup failed".into(),
        "nvme-temperature" => "the drive reports a temperature warning".into(),
        "nvme-media-errors" => format!("{v:.0} media / data integrity errors"),
        "worn" => format!("only {v:.0} % of its rated life left"),
        "ata-predict-failure" => "the drive itself predicts a failure".into(),
        "ata-threshold" => format!(
            "attribute {} fell below its threshold ({v:.0})",
            attr.unwrap_or_default()
        ),
        "ata-reallocated" => format!("{v:.0} reallocated sectors"),
        "ata-pending" => format!("{v:.0} sectors waiting to be remapped"),
        "ata-uncorrectable" => format!("{v:.0} uncorrectable sectors"),
        other => format!("{other} ({}: {v})", d.model),
    }
}

fn note_text(note: &str) -> &'static str {
    match note {
        "usb-bridge" => {
            "SMART is not readable through this USB bridge (try an administrator terminal)"
        }
        "needs-admin" => "thresholds need an administrator terminal",
        "raid" => "RAID / Storage Spaces: member drives are not readable",
        "virtual" => "virtual drive, no SMART",
        _ => "the drive reports no SMART / health data",
    }
}

fn hours(h: u64) -> String {
    if h >= 48 {
        format!("{h} h ({:.1} days)", h as f64 / 24.0)
    } else {
        format!("{h} h")
    }
}

fn print_text(w: &mut impl Write, r: &HealthReport, raw: bool) -> io::Result<()> {
    if !r.elevated {
        writeln!(
            w,
            "(standard user: SMART thresholds, USB bridges and the dirty bit need an administrator terminal)\n"
        )?;
    }
    for d in &r.drives {
        writeln!(
            w,
            "Disk {}  {}  [{}]  {}",
            d.index,
            d.model,
            d.bus,
            d.size.map(format_size).unwrap_or_default()
        )?;
        let mut line = format!("  Health: {}", status_word(d.status));
        if let Some(l) = d.life_left {
            line += &format!(" ({l} % life left)");
        }
        if let Some(t) = d.temperature {
            line += &format!("   Temperature: {t:.0} °C");
            if let Some(warn) = d.temp_warning {
                line += &format!(" (warning at {warn:.0} °C)");
            }
        }
        writeln!(w, "{line}")?;
        for reason in &d.reasons {
            writeln!(w, "    - {}", reason_text(d, reason))?;
        }
        if let Some(h) = d.power_on_hours {
            write!(w, "  Power on: {}", hours(h))?;
            if let Some(c) = d.power_cycles {
                write!(w, ", {c} power cycles")?;
            }
            writeln!(w)?;
        }
        if let Some(b) = d.bytes_written {
            write!(w, "  Written: {}", format_size(b))?;
            if let Some(rd) = d.bytes_read {
                write!(w, ", read: {}", format_size(rd))?;
            }
            writeln!(w)?;
        }
        if let Some(fw) = &d.firmware {
            write!(w, "  Firmware: {fw}")?;
            if let Some(s) = &d.serial {
                write!(w, "   Serial: {s}")?;
            }
            writeln!(w)?;
        }
        if !d.volumes.is_empty() {
            writeln!(w, "  Volumes: {}", d.volumes.join(" "))?;
        }
        if let Some(n) = &d.note {
            writeln!(w, "  Note: {}", note_text(n))?;
        }
        if raw {
            if let Some(n) = &d.nvme {
                writeln!(w, "  NVMe health log:")?;
                for (k, v) in [
                    ("Critical warning", format!("0x{:02X}", n.critical_warning)),
                    (
                        "Available spare",
                        format!(
                            "{} % (threshold {} %)",
                            n.available_spare, n.spare_threshold
                        ),
                    ),
                    ("Percentage used", format!("{} %", n.percentage_used)),
                    ("Data units read", n.data_units_read.to_string()),
                    ("Data units written", n.data_units_written.to_string()),
                    ("Host read commands", n.host_reads.to_string()),
                    ("Host write commands", n.host_writes.to_string()),
                    ("Controller busy time", format!("{} min", n.controller_busy)),
                    ("Unsafe shutdowns", n.unsafe_shutdowns.to_string()),
                    ("Media errors", n.media_errors.to_string()),
                    ("Error log entries", n.error_log_entries.to_string()),
                    (
                        "Warning temp. time",
                        format!("{} min", n.warning_temp_minutes),
                    ),
                    (
                        "Critical temp. time",
                        format!("{} min", n.critical_temp_minutes),
                    ),
                ] {
                    writeln!(w, "    {k:<22} {v}")?;
                }
            }
            if let Some(a) = &d.ata {
                writeln!(
                    w,
                    "  ID  Attribute                              Cur Wst Thr  Raw"
                )?;
                for at in &a.attributes {
                    writeln!(
                        w,
                        "  {:02X}  {:<38} {:>3} {:>3} {:>3}  {}",
                        at.id,
                        at.name().unwrap_or("Vendor specific"),
                        at.current,
                        at.worst,
                        at.threshold.map_or("-".into(), |t| t.to_string()),
                        at.raw
                    )?;
                }
            }
        }
        writeln!(w)?;
    }
    if !r.volumes.is_empty() {
        writeln!(
            w,
            "{:<5} {:<6} {:>9} {:>9} {:>6} {:>8} {:<6} {:<5} {:<10} {:<6} Full in",
            "Vol", "FS", "Size", "Free", "Free%", "Cluster", "Align", "TRIM", "BitLocker", "Dirty"
        )?;
        for v in &r.volumes {
            let yn = |b: Option<bool>| match b {
                Some(true) => "yes",
                Some(false) => "no",
                None => "-",
            };
            let full = match (v.has_trend, v.days_to_full) {
                (_, Some(d)) => format!("~{d:.0} days"),
                (true, None) => "not filling".into(),
                (false, None) => "no history".into(),
            };
            writeln!(
                w,
                "{:<5} {:<6} {:>9} {:>9} {:>5.1}% {:>8} {:<6} {:<5} {:<10} {:<6} {}",
                v.root,
                v.fs,
                format_size(v.total),
                format_size(v.free),
                v.free_percent(),
                v.cluster
                    .map(|c| format_size(u64::from(c)))
                    .unwrap_or_default(),
                yn(v.aligned),
                yn(v.trim_enabled),
                v.bitlocker.as_deref().unwrap_or("-"),
                if v.dirty.is_none() && !r.elevated {
                    "?"
                } else {
                    yn(v.dirty)
                },
                full
            )?;
        }
    }
    Ok(())
}
