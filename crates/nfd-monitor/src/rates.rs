//! The Counters engine: per-process `IO_COUNTERS` (bytes and operations, read and
//! write) turned into one interval's activity. Works without admin rights; counts
//! every I/O of a process (files, network, pipes) and knows no file names.

use std::collections::HashMap;

use crate::{Counts, ProcCounts, Tick};

/// Cumulative counters of one process, as the kernel reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cumulative {
    pub pid: u32,
    pub create_time: u64,
    pub counts: Counts,
}

#[derive(Debug, Default)]
pub struct CounterRates {
    prev: Option<HashMap<(u32, u64), Counts>>,
}

impl CounterRates {
    /// Activity since the previous call. The first call only sets the baseline. A
    /// process that started in between counts from zero (catches short-lived
    /// writers); a PID reused by another process is a different key.
    pub fn tick(&mut self, procs: &[Cumulative], elapsed_ms: u64) -> Tick {
        let next: HashMap<(u32, u64), Counts> = procs
            .iter()
            .map(|p| ((p.pid, p.create_time), p.counts))
            .collect();
        let mut out = Vec::new();
        if let Some(prev) = &self.prev {
            for p in procs {
                let before = prev
                    .get(&(p.pid, p.create_time))
                    .copied()
                    .unwrap_or_default();
                let d = p.counts.since(&before);
                if !d.is_zero() {
                    out.push(ProcCounts {
                        pid: p.pid,
                        create_time: p.create_time,
                        counts: d,
                    });
                }
            }
        }
        self.prev = Some(next);
        Tick {
            elapsed_ms,
            procs: out,
            ..Tick::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cum(pid: u32, ct: u64, read: u64, write: u64) -> Cumulative {
        Cumulative {
            pid,
            create_time: ct,
            counts: Counts {
                read,
                write,
                read_ops: read / 10,
                write_ops: write / 10,
            },
        }
    }

    #[test]
    fn deltas_between_samples() {
        let mut r = CounterRates::default();
        assert!(
            r.tick(&[cum(1, 10, 100, 100)], 0).procs.is_empty(),
            "baseline"
        );
        let t = r.tick(
            &[cum(1, 10, 150, 400), cum(2, 20, 30, 0), cum(3, 30, 0, 0)],
            1000,
        );
        assert_eq!(t.elapsed_ms, 1000);
        assert_eq!(t.procs.len(), 2, "idle process 3 left out");
        assert_eq!(t.procs[0].counts.read, 50);
        assert_eq!(t.procs[0].counts.write, 300);
        assert_eq!(t.procs[0].counts.write_ops, 30);
        assert_eq!(t.procs[1].counts.read, 30, "new process counts from zero");
    }

    #[test]
    fn reused_pid_is_a_new_process() {
        let mut r = CounterRates::default();
        r.tick(&[cum(1, 10, 1_000_000, 0)], 0);
        let t = r.tick(&[cum(1, 99, 5, 0)], 1000);
        assert_eq!(t.procs[0].counts.read, 5, "not 5 - 1 000 000, not skipped");
        assert_eq!(t.procs[0].create_time, 99);
    }
}
