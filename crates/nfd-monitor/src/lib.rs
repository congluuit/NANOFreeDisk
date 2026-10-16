//! Disk activity monitor (`docs/PHASE6_MONITOR.md`, PLAN §4.5).
//!
//! An *engine* produces a [`Tick`] — raw I/O counts of one sampling interval per
//! process (and per file, with ETW). [`Monitor::ingest`] turns ticks into rates,
//! joins them with the process list, keeps the realtime [`chart`], the per-minute
//! downsample for the 24-hour history ([`minute`]) and the log-spam detector
//! ([`spam`], PLAN §13.2 C8).
//!
//! Engines:
//! - [`rates::CounterRates`] — `IO_COUNTERS` deltas per process; no admin needed.
//! - ETW (`win::etw`, decoded by [`etw_parse`]) — file-level I/O from a private
//!   SystemTraceProvider session; needs admin, so the app runs it in the elevated
//!   helper.
//!
//! Everything here only *reads*: there is no API that ends, suspends or changes a
//! process. Ending a process is the Tasks view's guarded flow (`nfd_tasks::end_task`).

pub mod chart;
pub mod etw_parse;
pub mod minute;
pub mod rates;
pub mod spam;
#[cfg(windows)]
pub mod win;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use chart::{Chart, Point};
pub use minute::{Minute, TopProc};
pub use spam::{Alert, SpamConfig};

/// JSON schema id of `nfd monitor --json` lines.
pub const SCHEMA: &str = "nanofreedisk.monitor.v1";

/// Where the numbers come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// Per-process `IO_COUNTERS` (every I/O: files, network, pipes); no files.
    Counters,
    /// ETW in this process (it runs elevated).
    Etw,
    /// ETW in the elevated helper.
    Helper,
}

impl Source {
    /// File-level detail (files, top written, "who touched this?").
    pub fn has_files(self) -> bool {
        !matches!(self, Self::Counters)
    }
}

/// I/O counts of one interval.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub read: u64,
    pub write: u64,
    pub read_ops: u64,
    pub write_ops: u64,
}

impl Counts {
    pub fn add(&mut self, o: &Self) {
        self.read = self.read.saturating_add(o.read);
        self.write = self.write.saturating_add(o.write);
        self.read_ops = self.read_ops.saturating_add(o.read_ops);
        self.write_ops = self.write_ops.saturating_add(o.write_ops);
    }

    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }

    /// `b - a` per field, never negative (counters of a process only grow).
    pub fn since(&self, earlier: &Self) -> Self {
        Self {
            read: self.read.saturating_sub(earlier.read),
            write: self.write.saturating_sub(earlier.write),
            read_ops: self.read_ops.saturating_sub(earlier.read_ops),
            write_ops: self.write_ops.saturating_sub(earlier.write_ops),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcCounts {
    pub pid: u32,
    /// FILETIME of the process start, 0 when the engine does not know it (ETW).
    pub create_time: u64,
    pub counts: Counts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCounts {
    pub pid: u32,
    /// Win32 path (`C:\…`).
    pub path: String,
    pub counts: Counts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TouchOp {
    Create,
    Write,
    Delete,
    Rename,
}

/// A file event under the watched folder ("Who touched this?", PLAN §13.2 F7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Touch {
    /// Unix time, ms.
    pub at: u64,
    pub pid: u32,
    pub op: TouchOp,
    pub path: String,
    /// New name of a rename.
    pub target: Option<String>,
}

/// Raw activity of one sampling interval: what an engine hands over (and what
/// crosses the helper pipe).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tick {
    pub elapsed_ms: u64,
    pub procs: Vec<ProcCounts>,
    pub files: Vec<FileCounts>,
    pub touches: Vec<Touch>,
    /// File events decoded in this interval (a session that delivers none while
    /// the disks are busy is "silent").
    pub events: u64,
    /// Events ETW dropped / could not decode, since the session started.
    pub lost: u64,
    pub undecoded: u64,
}

impl Tick {
    /// Keep the `n` busiest files (bytes read + written). Process totals are not
    /// affected: they were summed before.
    pub fn trim_files(&mut self, n: usize) {
        if self.files.len() > n {
            self.files
                .sort_by_key(|f| std::cmp::Reverse(f.counts.read.saturating_add(f.counts.write)));
            self.files.truncate(n);
        }
    }
}

/// What the app knows about a live process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub create_time: u64,
    pub name: String,
    pub path: Option<String>,
}

/// One process's rates (bytes / s, operations / s).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcRate {
    pub pid: u32,
    /// FILETIME as a decimal string (beyond JavaScript's safe integers); empty when
    /// the process has exited.
    pub create_time: String,
    pub name: String,
    pub path: Option<String>,
    pub read: f64,
    pub write: f64,
    pub read_ops: f64,
    pub write_ops: f64,
    /// Files it touched in the interval (file-level sources only).
    pub files: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRate {
    pub pid: u32,
    pub process: String,
    pub path: String,
    pub read: f64,
    pub write: f64,
    pub ops: f64,
}

/// One physical disk (PDH `PhysicalDisk`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskStat {
    /// PDH instance, e.g. `0 C: D:`.
    pub name: String,
    pub read: f64,
    pub write: f64,
    pub transfers: f64,
    /// Average queue length.
    pub queue: f64,
    /// Average seconds per transfer, in ms.
    pub latency_ms: f64,
    /// % idle time (0–100).
    pub idle: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TouchRow {
    pub at: u64,
    pub pid: u32,
    pub process: String,
    pub op: TouchOp,
    pub path: String,
    pub target: Option<String>,
}

/// Everything the Monitor view shows for one interval.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub schema: &'static str,
    /// Unix time, ms.
    pub at: u64,
    pub source: Source,
    pub interval_ms: u64,
    /// Totals of all processes, bytes / s and operations / s.
    pub read: f64,
    pub write: f64,
    pub ops: f64,
    /// Active processes, busiest first.
    pub processes: Vec<ProcRate>,
    /// Files with I/O, most written first (file-level sources only).
    pub files: Vec<FileRate>,
    pub disks: Vec<DiskStat>,
    pub touches: Vec<TouchRow>,
    pub lost: u64,
    pub undecoded: u64,
    /// Alerts raised in this interval.
    pub alerts: Vec<Alert>,
}

/// Most processes / files a snapshot carries.
pub const MAX_PROCESSES: usize = 150;
pub const MAX_FILES: usize = 300;

/// `bytes` over `ms` as a per-second rate.
pub fn per_second(n: u64, ms: u64) -> f64 {
    if ms == 0 {
        0.0
    } else {
        n as f64 * 1000.0 / ms as f64
    }
}

/// Is `path` a log-like file that the big-log check looks at?
pub fn is_log_like(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".log", ".txt", ".etl", ".jsonl", ".out", ".trace"]
        .iter()
        .any(|e| lower.ends_with(e))
        || lower.contains(".log.")
}

/// Safety net for the file-level sources: a trace session that delivers no file
/// event for 10 s while the disks moved more than 1 MB is "silent" (blocked by
/// security software, unknown event layouts…) and the caller falls back to the
/// Counters engine.
#[derive(Debug, Default)]
pub struct SilentCheck {
    quiet_ms: u64,
    disk_bytes: f64,
}

impl SilentCheck {
    pub fn observe(&mut self, tick: &Tick, disks: &[DiskStat]) -> bool {
        if tick.events > 0 {
            *self = Self::default();
            return false;
        }
        self.quiet_ms += tick.elapsed_ms;
        let rate: f64 = disks.iter().map(|d| d.read + d.write).sum();
        self.disk_bytes += rate * tick.elapsed_ms as f64 / 1000.0;
        self.quiet_ms >= 10_000 && self.disk_bytes >= 1024.0 * 1024.0
    }
}

/// Turns ticks into snapshots; keeps the chart, the minute downsample and alerts.
#[derive(Debug, Default)]
pub struct Monitor {
    chart: Chart,
    minutes: minute::MinuteAcc,
    spam: spam::SpamWatch,
    logs: spam::LogWatch,
}

impl Monitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_alerts(&mut self, cfg: SpamConfig) {
        self.spam.set_config(cfg);
        self.logs.enabled = cfg.enabled;
    }

    pub fn alerts(&self) -> SpamConfig {
        self.spam.config()
    }

    /// The realtime chart, last `seconds` points (60 or 600).
    pub fn chart(&self, seconds: usize) -> Vec<Point> {
        self.chart.last(seconds)
    }

    /// Minutes completed since the last call (for the 24-hour history).
    pub fn take_minutes(&mut self) -> Vec<Minute> {
        self.minutes.take()
    }

    /// The minute in progress (written on exit).
    pub fn flush_minute(&mut self) -> Option<Minute> {
        self.minutes.flush()
    }

    /// One interval. `procs` is the live process list (names, paths); `file_size`
    /// reads a file's size for the big-log check (metadata only).
    pub fn ingest(
        &mut self,
        now_ms: u64,
        source: Source,
        tick: &Tick,
        procs: &[ProcInfo],
        disks: Vec<DiskStat>,
        file_size: &dyn Fn(&str) -> Option<u64>,
    ) -> Snapshot {
        let by_pid: HashMap<u32, &ProcInfo> = procs.iter().map(|p| (p.pid, p)).collect();
        let ms = tick.elapsed_ms;
        // one row per (pid, create time); the engines already send one, merge defensively
        let mut merged: HashMap<(u32, u64), Counts> = HashMap::new();
        for p in &tick.procs {
            merged
                .entry((p.pid, p.create_time))
                .or_default()
                .add(&p.counts);
        }
        let mut files_per_pid: HashMap<u32, u32> = HashMap::new();
        let mut files: HashMap<(u32, &str), Counts> = HashMap::new();
        for f in &tick.files {
            let e = files.entry((f.pid, f.path.as_str())).or_default();
            if e.is_zero() {
                *files_per_pid.entry(f.pid).or_default() += 1;
            }
            e.add(&f.counts);
        }
        let lookup = |pid: u32, ct: u64| -> Option<&ProcInfo> {
            by_pid
                .get(&pid)
                .copied()
                .filter(|i| ct == 0 || i.create_time == ct)
        };
        let name_of = |pid: u32, info: Option<&ProcInfo>| -> String {
            match (pid, info) {
                (_, Some(i)) if !i.name.is_empty() => i.name.clone(),
                (0, _) => "System Idle Process".into(),
                (4, _) => "System".into(),
                _ => format!("PID {pid}"),
            }
        };

        let mut total = Counts::default();
        let mut processes = Vec::with_capacity(merged.len());
        let mut by_name: HashMap<String, Counts> = HashMap::new();
        for (&(pid, ct), c) in &merged {
            if c.is_zero() {
                continue;
            }
            total.add(c);
            let info = lookup(pid, ct);
            let name = name_of(pid, info);
            by_name.entry(name.clone()).or_default().add(c);
            processes.push(ProcRate {
                pid,
                create_time: info.map(|i| i.create_time.to_string()).unwrap_or_default(),
                name,
                path: info.and_then(|i| i.path.clone()),
                read: per_second(c.read, ms),
                write: per_second(c.write, ms),
                read_ops: per_second(c.read_ops, ms),
                write_ops: per_second(c.write_ops, ms),
                files: files_per_pid.get(&pid).copied().unwrap_or(0),
            });
        }
        processes.sort_by(|a, b| {
            (b.read + b.write)
                .total_cmp(&(a.read + a.write))
                .then(a.pid.cmp(&b.pid))
        });
        processes.truncate(MAX_PROCESSES);

        let mut file_rows: Vec<FileRate> = files
            .iter()
            .map(|(&(pid, path), c)| FileRate {
                pid,
                process: name_of(pid, lookup(pid, 0)),
                path: path.to_owned(),
                read: per_second(c.read, ms),
                write: per_second(c.write, ms),
                ops: per_second(c.read_ops + c.write_ops, ms),
            })
            .collect();
        file_rows.sort_by(|a, b| {
            b.write
                .total_cmp(&a.write)
                .then(b.read.total_cmp(&a.read))
                .then(a.path.cmp(&b.path))
        });
        file_rows.truncate(MAX_FILES);

        let read = per_second(total.read, ms);
        let write = per_second(total.write, ms);
        self.chart.push(Point {
            t: now_ms / 1000,
            read: read as f32,
            write: write as f32,
        });
        self.minutes
            .add(now_ms, &total, read, write, by_name.into_iter());

        let mut alerts = self.spam.observe(now_ms, &processes);
        alerts.extend(self.logs.observe(now_ms, &file_rows, file_size));

        let touches = tick
            .touches
            .iter()
            .map(|t| TouchRow {
                at: t.at,
                pid: t.pid,
                process: name_of(t.pid, lookup(t.pid, 0)),
                op: t.op,
                path: t.path.clone(),
                target: t.target.clone(),
            })
            .collect();

        Snapshot {
            schema: SCHEMA,
            at: now_ms,
            source,
            interval_ms: ms,
            read,
            write,
            ops: per_second(total.read_ops + total.write_ops, ms),
            processes,
            files: file_rows,
            disks,
            touches,
            lost: tick.lost,
            undecoded: tick.undecoded,
            alerts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(read: u64, write: u64) -> Counts {
        Counts {
            read,
            write,
            read_ops: 1,
            write_ops: 1,
        }
    }

    fn info(pid: u32, ct: u64, name: &str) -> ProcInfo {
        ProcInfo {
            pid,
            create_time: ct,
            name: name.into(),
            path: Some(format!("C:\\bin\\{name}")),
        }
    }

    #[test]
    fn rates_are_per_second_and_sorted() {
        let tick = Tick {
            elapsed_ms: 500,
            procs: vec![
                ProcCounts {
                    pid: 10,
                    create_time: 100,
                    counts: counts(1000, 0),
                },
                ProcCounts {
                    pid: 20,
                    create_time: 200,
                    counts: counts(0, 4000),
                },
                ProcCounts {
                    pid: 30,
                    create_time: 300,
                    counts: Counts::default(),
                },
            ],
            ..Tick::default()
        };
        let procs = [
            info(10, 100, "a.exe"),
            info(20, 200, "b.exe"),
            info(30, 300, "c.exe"),
        ];
        let mut m = Monitor::new();
        let s = m.ingest(10_000, Source::Counters, &tick, &procs, vec![], &|_| None);
        assert_eq!(s.processes.len(), 2, "idle processes are left out");
        assert_eq!(s.processes[0].name, "b.exe");
        assert_eq!(s.processes[0].write, 8000.0);
        assert_eq!(s.processes[1].read, 2000.0);
        assert_eq!(s.processes[1].read_ops, 2.0);
        assert_eq!((s.read, s.write), (2000.0, 8000.0));
        assert_eq!(s.processes[0].create_time, "200");
        assert_eq!(m.chart(60).len(), 1);
    }

    #[test]
    fn a_reused_pid_does_not_borrow_the_new_process_name() {
        let tick = Tick {
            elapsed_ms: 1000,
            procs: vec![ProcCounts {
                pid: 10,
                create_time: 100,
                counts: counts(1, 1),
            }],
            ..Tick::default()
        };
        let s = Monitor::new().ingest(
            0,
            Source::Counters,
            &tick,
            &[info(10, 999, "new.exe")],
            vec![],
            &|_| None,
        );
        assert_eq!(s.processes[0].name, "PID 10");
        assert_eq!(s.processes[0].create_time, "");
    }

    #[test]
    fn files_are_counted_per_process_and_most_written_first() {
        let f = |pid, path: &str, c| FileCounts {
            pid,
            path: path.into(),
            counts: c,
        };
        let tick = Tick {
            elapsed_ms: 1000,
            procs: vec![ProcCounts {
                pid: 10,
                create_time: 0,
                counts: counts(10, 300),
            }],
            files: vec![
                f(10, r"C:\a.log", counts(0, 100)),
                f(10, r"C:\b.dat", counts(10, 0)),
                f(10, r"C:\a.log", counts(0, 200)),
            ],
            ..Tick::default()
        };
        let s = Monitor::new().ingest(
            0,
            Source::Etw,
            &tick,
            &[info(10, 5, "x.exe")],
            vec![],
            &|_| None,
        );
        assert_eq!(s.processes[0].files, 2);
        assert_eq!(s.processes[0].name, "x.exe", "create time 0 matches any");
        assert_eq!(s.files[0].path, r"C:\a.log");
        assert_eq!(s.files[0].write, 300.0);
        assert_eq!(s.files[0].process, "x.exe");
    }

    #[test]
    fn silent_sessions_are_noticed_only_when_the_disks_were_busy() {
        let disk = |rate: f64| DiskStat {
            name: "0 C:".into(),
            read: 0.0,
            write: rate,
            transfers: 0.0,
            queue: 0.0,
            latency_ms: 0.0,
            idle: 100.0,
        };
        let quiet = Tick {
            elapsed_ms: 1000,
            ..Tick::default()
        };
        let mut s = SilentCheck::default();
        for _ in 0..20 {
            assert!(
                !s.observe(&quiet, &[disk(10.0)]),
                "idle disks: nothing to see"
            );
        }
        let mut s = SilentCheck::default();
        let mut silent = false;
        for i in 0..10 {
            silent = s.observe(&quiet, &[disk(1e6)]);
            assert!(!silent || i == 9);
        }
        assert!(silent);
        let busy = Tick {
            elapsed_ms: 1000,
            events: 5,
            ..Tick::default()
        };
        assert!(!s.observe(&busy, &[disk(1e6)]));
        assert!(!s.observe(&quiet, &[disk(1e6)]), "counting starts over");
    }

    #[test]
    fn log_like_names() {
        assert!(is_log_like(r"C:\x\app.LOG"));
        assert!(is_log_like(r"C:\x\app.log.1"));
        assert!(is_log_like(r"C:\x\trace.etl"));
        assert!(!is_log_like(r"C:\x\movie.mkv"));
    }
}
