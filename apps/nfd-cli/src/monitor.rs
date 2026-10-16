//! `nfd monitor` — live disk activity per process (docs/PHASE6_MONITOR.md §2.4).
//! Read-only: prints what reads and writes the disks; never touches a process.

use std::process::ExitCode;
use std::time::Duration;

use clap::Args;

#[derive(Args, Debug)]
pub struct MonitorArgs {
    /// Stop after this many seconds (0 = until Ctrl+C).
    #[arg(long, default_value_t = 10)]
    seconds: u64,
    /// Seconds between samples.
    #[arg(long, default_value_t = 1)]
    interval: u64,
    /// Processes (and files) printed per sample.
    #[arg(long, default_value_t = 10)]
    top: usize,
    /// One JSON object per sample (`nanofreedisk.monitor.v1`, JSON Lines).
    #[arg(long)]
    json: bool,
}

pub(crate) fn rate(b: f64) -> String {
    const U: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut v = b;
    let mut i = 0;
    while v >= 1024.0 && i + 1 < U.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", U[i])
    } else {
        format!("{v:.1} {}", U[i])
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn print_snapshot(s: &nfd_monitor::Snapshot, top: usize) {
    let secs = s.at / 1000;
    println!(
        "{:02}:{:02}:{:02} UTC  read {}  write {}  {:.0} ops/s  [{}]",
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60,
        rate(s.read),
        rate(s.write),
        s.ops,
        serde_json::to_value(s.source)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    );
    for p in s.processes.iter().take(top) {
        println!(
            "  {:>7}  {:<28} R {:>11}  W {:>11}  {:>7.0} IOPS{}",
            p.pid,
            p.name.chars().take(28).collect::<String>(),
            rate(p.read),
            rate(p.write),
            p.read_ops + p.write_ops,
            if p.files > 0 {
                format!("  {} file(s)", p.files)
            } else {
                String::new()
            }
        );
    }
    for f in s.files.iter().take(top) {
        println!(
            "    W {:>11}  R {:>11}  {}  ({})",
            rate(f.write),
            rate(f.read),
            f.path,
            f.process
        );
    }
    for d in &s.disks {
        println!(
            "  disk {:<12} R {:>11}  W {:>11}  queue {:.2}  {:.1} ms  idle {:.0} %",
            d.name,
            rate(d.read),
            rate(d.write),
            d.queue,
            d.latency_ms,
            d.idle
        );
    }
    for a in &s.alerts {
        if let Ok(j) = serde_json::to_string(a) {
            println!("  ALERT {j}");
        }
    }
}

#[cfg(windows)]
pub fn run(args: MonitorArgs) -> anyhow::Result<ExitCode> {
    let mut sampler = nfd_monitor::win::Sampler::new();
    let mut mon = nfd_monitor::Monitor::new();
    let interval = Duration::from_secs(args.interval.max(1));
    let rounds = if args.seconds == 0 {
        u64::MAX
    } else {
        (args.seconds / args.interval.max(1)).max(1)
    };
    let size = |p: &str| std::fs::metadata(p).ok().map(|m| m.len());
    for _ in 0..rounds {
        std::thread::sleep(interval);
        let s = sampler.sample(None);
        let snap = mon.ingest(unix_ms(), s.source, &s.tick, &s.procs, s.disks, &size);
        if args.json {
            println!("{}", serde_json::to_string(&snap)?);
        } else {
            print_snapshot(&snap, args.top);
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(not(windows))]
pub fn run(_args: MonitorArgs) -> anyhow::Result<ExitCode> {
    let _ = Duration::ZERO;
    anyhow::bail!("nfd monitor is Windows-only for now")
}

#[cfg(test)]
mod tests {
    #[test]
    fn rates_are_readable() {
        assert_eq!(super::rate(512.0), "512 B/s");
        assert_eq!(super::rate(1536.0), "1.5 KB/s");
        assert_eq!(super::rate(5.0 * 1024.0 * 1024.0), "5.0 MB/s");
    }
}
