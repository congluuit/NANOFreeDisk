//! `nfd sensors` — live sensors (docs/HARDWARE_MONITOR.md).

use std::process::ExitCode;
use std::time::Duration;

use clap::Args;
use nfd_sensors::{Kind, SensorHub, Snapshot};

#[derive(Args, Debug)]
pub struct SensorsArgs {
    /// Keep sampling once a second and redraw (Ctrl+C to stop).
    #[arg(long)]
    watch: bool,
    /// Print the snapshot as JSON.
    #[arg(long)]
    json: bool,
    /// Only the main sensors (CPU / GPU / disks / battery).
    #[arg(long)]
    simple: bool,
    /// Add Advanced sensors through PawnIO (CPU temperatures, power, VID, clocks).
    /// Needs an elevated prompt and the PawnIO driver; read-only.
    #[arg(long)]
    advanced: bool,
}

/// "1.5 GB", "12.3 MB/s"
fn bytes(v: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = v;
    let mut u = 0;
    while v >= 1024.0 && u < units.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", units[u])
}

pub fn value(kind: Kind, v: f64) -> String {
    match kind {
        Kind::Temperature => format!("{v:.1} °C"),
        Kind::Voltage => format!("{v:.3} V"),
        Kind::Power => format!("{v:.1} W"),
        Kind::Clock => format!("{v:.0} MHz"),
        Kind::Load | Kind::Level | Kind::Control => format!("{v:.1} %"),
        Kind::Fan => format!("{v:.0} RPM"),
        Kind::Data => bytes(v),
        Kind::Throughput => format!("{}/s", bytes(v)),
        Kind::Energy => format!("{v:.0} mWh"),
    }
}

fn label(s: &nfd_sensors::SensorValue) -> String {
    let mut l = s.info.label.clone();
    if let Some(i) = s.info.index {
        l.push_str(&format!(" #{i}"));
    }
    if let Some(d) = &s.info.detail {
        l.push_str(&format!(" ({d})"));
    }
    if s.info.estimated {
        l.push_str(" ~");
    }
    l
}

fn print(snap: &Snapshot, simple: bool) {
    let dash = |v: Option<f64>, k| v.map_or("-".to_owned(), |v| value(k, v));
    for dev in &snap.devices {
        let rows: Vec<_> = snap
            .sensors
            .iter()
            .filter(|s| s.info.device == dev.id && (!simple || s.info.primary))
            .collect();
        if rows.is_empty() {
            continue;
        }
        println!("{}", dev.name);
        for s in rows {
            println!(
                "  {:<28} {:>14} {:>14} {:>14}",
                label(s),
                dash(s.value, s.info.kind),
                dash(s.min, s.info.kind),
                dash(s.max, s.info.kind)
            );
        }
        for n in snap.notes.iter().filter(|n| n.device == dev.id) {
            println!(
                "  {:<28} {}",
                format!("{:?}", n.kind).to_lowercase(),
                n.reason
            );
        }
    }
    println!(
        "(value / min / max; ~ = estimated; sampled in {:.1} ms)",
        snap.sample_ms
    );
}

#[cfg(windows)]
fn add_advanced(hub: &mut SensorHub) -> anyhow::Result<()> {
    let st = nfd_sensors::win::pawnio::status();
    if !st.installed {
        anyhow::bail!("PawnIO is not installed (https://pawnio.eu)");
    }
    match nfd_sensors::win::pawnio_providers() {
        Ok(list) => {
            for p in list {
                hub.add_provider(p);
            }
            Ok(())
        }
        Err(e) => anyhow::bail!("Advanced sensors unavailable: {e:?} ({st:?})"),
    }
}

#[cfg(not(windows))]
fn add_advanced(_hub: &mut SensorHub) -> anyhow::Result<()> {
    anyhow::bail!("Advanced sensors are Windows-only")
}

pub fn run(args: SensorsArgs) -> anyhow::Result<ExitCode> {
    let mut hub = SensorHub::native();
    if args.advanced {
        add_advanced(&mut hub)?;
    }
    hub.sample();
    // rates and loads need two samples
    std::thread::sleep(Duration::from_millis(1000));
    hub.sample();
    if args.json {
        println!("{}", serde_json::to_string_pretty(&hub.snapshot())?);
        return Ok(ExitCode::SUCCESS);
    }
    if !args.watch {
        print(&hub.snapshot(), args.simple);
        return Ok(ExitCode::SUCCESS);
    }
    loop {
        print!("\x1b[2J\x1b[H");
        print(&hub.snapshot(), args.simple);
        std::thread::sleep(Duration::from_millis(1000));
        hub.sample();
    }
}
