//! Log-spam detector (PLAN §13.2 C8): a process that keeps writing faster than
//! X MB/s for Y minutes, and log files over 1 GB that are being written.
//!
//! Only reports — the user decides what to do (open the folder, look the process
//! up in Tasks). Nothing here ends or throttles a process.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{FileRate, ProcRate, is_log_like};

/// 1 MB as the UI shows it (binary, like the rest of the app).
pub const MB: f64 = 1024.0 * 1024.0;
/// A log file this large that is still being written is reported.
pub const BIG_LOG: u64 = 1 << 30;
/// Short dips below the threshold do not break a run.
const GRACE_MS: u64 = 5_000;
/// After an alert, the process must stay below the threshold this long before it
/// can alert again.
const REARM_MS: u64 = 60_000;
/// A log file's size is read at most this often.
const SIZE_EVERY_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpamConfig {
    pub enabled: bool,
    /// Threshold in MB/s.
    pub mbps: f64,
    /// For this many minutes without a break.
    pub minutes: u32,
}

impl Default for SpamConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mbps: 5.0,
            minutes: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Alert {
    /// Wrote faster than the threshold for `minutes` minutes.
    LogSpam {
        pid: u32,
        create_time: String,
        name: String,
        path: Option<String>,
        /// Average write rate over the run, bytes / s.
        rate: f64,
        minutes: u32,
        /// Unix ms when the run started.
        since: u64,
    },
    /// A log-like file over 1 GB is being written.
    BigLog {
        pid: u32,
        name: String,
        path: String,
        size: u64,
    },
}

#[derive(Debug)]
struct Run {
    since: u64,
    last_over: u64,
    sum: f64,
    samples: u32,
    alerted: bool,
}

#[derive(Debug, Default)]
pub struct SpamWatch {
    cfg: SpamConfig,
    runs: HashMap<(u32, String), Run>,
}

impl SpamWatch {
    pub fn set_config(&mut self, cfg: SpamConfig) {
        if cfg != self.cfg {
            self.runs.clear();
        }
        self.cfg = cfg;
    }

    pub fn config(&self) -> SpamConfig {
        self.cfg
    }

    pub fn observe(&mut self, now: u64, procs: &[ProcRate]) -> Vec<Alert> {
        if !self.cfg.enabled || self.cfg.mbps <= 0.0 {
            self.runs.clear();
            return Vec::new();
        }
        let limit = self.cfg.mbps * MB;
        let need = u64::from(self.cfg.minutes.max(1)) * 60_000;
        let mut out = Vec::new();
        for p in procs.iter().filter(|p| p.write >= limit) {
            let key = (p.pid, p.create_time.clone());
            let run = self.runs.entry(key).or_insert(Run {
                since: now,
                last_over: now,
                sum: 0.0,
                samples: 0,
                alerted: false,
            });
            if now.saturating_sub(run.last_over) > GRACE_MS && !run.alerted {
                // the previous run broke off: start over
                *run = Run {
                    since: now,
                    last_over: now,
                    sum: 0.0,
                    samples: 0,
                    alerted: false,
                };
            }
            run.last_over = now;
            run.sum += p.write;
            run.samples += 1;
            if !run.alerted && now.saturating_sub(run.since) >= need {
                run.alerted = true;
                out.push(Alert::LogSpam {
                    pid: p.pid,
                    create_time: p.create_time.clone(),
                    name: p.name.clone(),
                    path: p.path.clone(),
                    rate: run.sum / f64::from(run.samples.max(1)),
                    minutes: self.cfg.minutes,
                    since: run.since,
                });
            }
        }
        self.runs.retain(|_, r| {
            let quiet = now.saturating_sub(r.last_over);
            if r.alerted {
                quiet < REARM_MS
            } else {
                quiet <= GRACE_MS
            }
        });
        out
    }
}

#[derive(Debug, Default)]
pub struct LogWatch {
    pub enabled: bool,
    checked: HashMap<String, u64>,
    reported: HashSet<String>,
}

impl LogWatch {
    pub fn observe(
        &mut self,
        now: u64,
        files: &[FileRate],
        size_of: &dyn Fn(&str) -> Option<u64>,
    ) -> Vec<Alert> {
        if !self.enabled {
            return Vec::new();
        }
        if self.checked.len() > 10_000 {
            self.checked.clear();
        }
        let mut out = Vec::new();
        for f in files
            .iter()
            .filter(|f| f.write > 0.0 && is_log_like(&f.path))
        {
            if self.reported.contains(&f.path) {
                continue;
            }
            if self
                .checked
                .get(&f.path)
                .is_some_and(|&t| now.saturating_sub(t) < SIZE_EVERY_MS)
            {
                continue;
            }
            self.checked.insert(f.path.clone(), now);
            if let Some(size) = size_of(&f.path).filter(|&s| s > BIG_LOG) {
                self.reported.insert(f.path.clone());
                out.push(Alert::BigLog {
                    pid: f.pid,
                    name: f.process.clone(),
                    path: f.path.clone(),
                    size,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, write_mb: f64) -> ProcRate {
        ProcRate {
            pid,
            create_time: format!("{pid}00"),
            name: format!("p{pid}.exe"),
            path: None,
            read: 0.0,
            write: write_mb * MB,
            read_ops: 0.0,
            write_ops: 0.0,
            files: 0,
        }
    }

    fn watch(minutes: u32) -> SpamWatch {
        let mut w = SpamWatch::default();
        w.set_config(SpamConfig {
            enabled: true,
            mbps: 5.0,
            minutes,
        });
        w
    }

    #[test]
    fn alerts_once_after_y_minutes_of_sustained_writes() {
        let mut w = watch(2);
        let mut alerts = Vec::new();
        for s in 0..=240u64 {
            alerts.extend(w.observe(s * 1000, &[proc(1, 8.0), proc(2, 1.0)]));
        }
        assert_eq!(alerts.len(), 1, "{alerts:?}");
        match &alerts[0] {
            Alert::LogSpam {
                pid, rate, since, ..
            } => {
                assert_eq!(*pid, 1);
                assert!((rate - 8.0 * MB).abs() < 1.0);
                assert_eq!(*since, 0);
            }
            a => panic!("{a:?}"),
        }
    }

    #[test]
    fn a_break_starts_over_but_short_dips_do_not() {
        let mut w = watch(1);
        let mut n = 0;
        // 50 s over, 3 s dip, 20 s over → 73 s run: alerts
        for s in 0..50u64 {
            n += w.observe(s * 1000, &[proc(1, 6.0)]).len();
        }
        for s in 50..53u64 {
            n += w.observe(s * 1000, &[proc(1, 0.5)]).len();
        }
        for s in 53..73u64 {
            n += w.observe(s * 1000, &[proc(1, 6.0)]).len();
        }
        assert_eq!(n, 1);
        // a different process: 40 s over, 30 s quiet, 40 s over → never a full minute
        let mut n = 0;
        for s in 0..40u64 {
            n += w.observe(1_000_000 + s * 1000, &[proc(2, 6.0)]).len();
        }
        for s in 70..110u64 {
            n += w.observe(1_000_000 + s * 1000, &[proc(2, 6.0)]).len();
        }
        assert_eq!(n, 0);
    }

    #[test]
    fn rearms_after_a_quiet_minute_and_off_means_off() {
        let mut w = watch(1);
        let mut n = 0;
        for s in 0..=60u64 {
            n += w.observe(s * 1000, &[proc(1, 6.0)]).len();
        }
        // quiet for 2 minutes
        for s in 61..181u64 {
            n += w.observe(s * 1000, &[]).len();
        }
        for s in 181..=241u64 {
            n += w.observe(s * 1000, &[proc(1, 6.0)]).len();
        }
        assert_eq!(n, 2);
        let mut off = SpamWatch::default();
        for s in 0..400u64 {
            assert!(off.observe(s * 1000, &[proc(1, 100.0)]).is_empty());
        }
    }

    #[test]
    fn big_logs_are_reported_once_and_sizes_read_sparingly() {
        let file = |path: &str, write| FileRate {
            pid: 7,
            process: "svc.exe".into(),
            path: path.into(),
            read: 0.0,
            write,
            ops: 1.0,
        };
        let reads = std::cell::Cell::new(0);
        let size = |p: &str| {
            reads.set(reads.get() + 1);
            Some(if p.ends_with("big.log") { 2 << 30 } else { 10 })
        };
        let mut w = LogWatch {
            enabled: true,
            ..LogWatch::default()
        };
        let rows = [
            file(r"C:\l\big.log", 100.0),
            file(r"C:\l\small.log", 100.0),
            file(r"C:\l\video.mp4", 100.0),
            file(r"C:\l\idle.log", 0.0),
        ];
        let a = w.observe(0, &rows, &size);
        assert_eq!(a.len(), 1);
        assert!(matches!(&a[0], Alert::BigLog { size, .. } if *size == 2 << 30));
        assert_eq!(reads.get(), 2, "only written log-like files");
        assert!(w.observe(1000, &rows, &size).is_empty());
        assert_eq!(reads.get(), 2, "small.log not re-read within a minute");
        w.observe(61_000, &rows, &size);
        assert_eq!(reads.get(), 3);
    }

    #[test]
    fn alerts_serialize_with_a_kind_tag() {
        let a = Alert::BigLog {
            pid: 1,
            name: "a".into(),
            path: "p".into(),
            size: 2,
        };
        let v = serde_json::to_value(&a).expect("json");
        assert_eq!(v["kind"], "big-log");
        let a = Alert::LogSpam {
            pid: 1,
            create_time: "5".into(),
            name: "a".into(),
            path: None,
            rate: 1.0,
            minutes: 5,
            since: 0,
        };
        let v = serde_json::to_value(&a).expect("json");
        assert_eq!(v["kind"], "log-spam");
        assert_eq!(v["createTime"], "5");
    }
}
