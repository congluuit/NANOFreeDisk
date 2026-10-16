//! `nfd ps` — running processes (docs/HARDWARE_MONITOR.md). Read-only: the CLI never
//! ends processes; End task exists only in the app, behind its confirmations.

use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Sort {
    Cpu,
    Memory,
    Io,
    Gpu,
    Name,
    Pid,
}

#[derive(Args, Debug)]
pub struct PsArgs {
    /// Sort order (descending except name / pid).
    #[arg(long, value_enum, default_value = "cpu")]
    sort: Sort,
    /// How many rows to print (0 = all).
    #[arg(long, default_value_t = 30)]
    limit: usize,
    /// Print the snapshot as JSON.
    #[arg(long)]
    json: bool,
}

#[cfg(windows)]
pub fn run(args: PsArgs) -> anyhow::Result<ExitCode> {
    let mut mon = nfd_tasks::TaskMonitor::new();
    mon.snapshot();
    // rates need two samples
    std::thread::sleep(Duration::from_millis(1000));
    let mut snap = mon.snapshot();
    let t = &mut snap.tasks;
    match args.sort {
        Sort::Cpu => t.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
        Sort::Memory => t.sort_by_key(|x| std::cmp::Reverse(x.memory)),
        Sort::Io => t.sort_by(|a, b| (b.io_read + b.io_write).total_cmp(&(a.io_read + a.io_write))),
        Sort::Gpu => t.sort_by(|a, b| b.gpu.total_cmp(&a.gpu)),
        Sort::Name => t.sort_by_key(|x| x.name.to_ascii_lowercase()),
        Sort::Pid => t.sort_by_key(|x| x.pid),
    }
    if args.limit > 0 {
        t.truncate(args.limit);
    }
    for x in t.iter_mut().filter(|x| x.publisher.is_none()) {
        x.publisher = x.path.as_deref().and_then(nfd_tasks::publisher_label);
    }
    if args.json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(ExitCode::SUCCESS);
    }
    let s = &snap.summary;
    println!(
        "CPU {:.1} %  ·  RAM {:.1} / {:.1} GB  ·  Disk {:.1} MB/s  ·  GPU {:.1} %  ·  {} processes",
        s.cpu,
        s.memory_used as f64 / f64::from(1u32 << 30),
        s.memory_total as f64 / f64::from(1u32 << 30),
        s.disk / 1e6,
        s.gpu,
        s.processes
    );
    println!(
        "{:>7}  {:<28} {:>6} {:>10} {:>10} {:>6}  {:<10} PUBLISHER",
        "PID", "NAME", "CPU%", "MEMORY", "I/O", "GPU%", "GROUP"
    );
    for x in &snap.tasks {
        println!(
            "{:>7}  {:<28} {:>6.1} {:>8.1} MB {:>7.1} KB/s {:>5.1}  {:<10} {}",
            x.pid,
            x.name.chars().take(28).collect::<String>(),
            x.cpu,
            x.memory as f64 / 1_048_576.0,
            (x.io_read + x.io_write) / 1024.0,
            x.gpu,
            format!("{:?}", x.group).to_lowercase(),
            x.publisher.as_deref().unwrap_or("")
        );
    }
    println!("(* = publisher from the version resource, signature not verified)");
    Ok(ExitCode::SUCCESS)
}

#[cfg(not(windows))]
pub fn run(_args: PsArgs) -> anyhow::Result<ExitCode> {
    let _ = Duration::ZERO;
    anyhow::bail!("nfd ps is Windows-only for now")
}
