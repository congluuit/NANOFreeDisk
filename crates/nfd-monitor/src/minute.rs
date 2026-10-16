//! Per-minute downsample for the 24-hour history (stored by the app / CLI in
//! `nfd-history`): bytes read / written, peak rates and the busiest processes.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Counts;

/// Processes kept per minute (by bytes written, then by bytes read).
pub const TOP: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopProc {
    pub name: String,
    pub read: u64,
    pub write: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Minute {
    /// Unix time of the minute's start, seconds.
    pub ts: u64,
    pub read: u64,
    pub write: u64,
    /// Highest per-second rates seen in the minute.
    pub peak_read: f64,
    pub peak_write: f64,
    /// Seconds actually sampled (the monitor may run only part of a minute).
    pub seconds: u32,
    pub top: Vec<TopProc>,
}

#[derive(Debug, Default)]
pub struct MinuteAcc {
    current: Option<Minute>,
    names: HashMap<String, Counts>,
    done: Vec<Minute>,
}

impl MinuteAcc {
    /// Add one sample taken at `now_ms` (its counts and rates; `by_name` = counts
    /// per process name).
    pub fn add(
        &mut self,
        now_ms: u64,
        total: &Counts,
        read_rate: f64,
        write_rate: f64,
        by_name: impl Iterator<Item = (String, Counts)>,
    ) {
        let ts = now_ms / 60_000 * 60;
        if self.current.as_ref().is_some_and(|m| m.ts != ts) {
            if let Some(m) = self.flush() {
                self.done.push(m);
            }
        }
        let m = self.current.get_or_insert_with(|| Minute {
            ts,
            read: 0,
            write: 0,
            peak_read: 0.0,
            peak_write: 0.0,
            seconds: 0,
            top: Vec::new(),
        });
        m.read = m.read.saturating_add(total.read);
        m.write = m.write.saturating_add(total.write);
        m.peak_read = m.peak_read.max(read_rate);
        m.peak_write = m.peak_write.max(write_rate);
        m.seconds += 1;
        for (name, c) in by_name {
            self.names.entry(name).or_default().add(&c);
        }
    }

    /// Close the minute in progress.
    pub fn flush(&mut self) -> Option<Minute> {
        let mut m = self.current.take()?;
        let mut names: Vec<(String, Counts)> = self.names.drain().collect();
        names.sort_by(|a, b| {
            b.1.write
                .cmp(&a.1.write)
                .then(b.1.read.cmp(&a.1.read))
                .then(a.0.cmp(&b.0))
        });
        m.top = names
            .into_iter()
            .filter(|(_, c)| c.read + c.write > 0)
            .take(TOP)
            .map(|(name, c)| TopProc {
                name,
                read: c.read,
                write: c.write,
            })
            .collect();
        Some(m)
    }

    pub fn take(&mut self) -> Vec<Minute> {
        std::mem::take(&mut self.done)
    }
}

/// Bytes written per process name over many minutes, most first (the 24-hour
/// "top writers" list).
pub fn top_writers(minutes: &[Minute], n: usize) -> Vec<TopProc> {
    let mut acc: HashMap<&str, (u64, u64)> = HashMap::new();
    for m in minutes {
        for t in &m.top {
            let e = acc.entry(t.name.as_str()).or_default();
            e.0 += t.read;
            e.1 += t.write;
        }
    }
    let mut v: Vec<TopProc> = acc
        .into_iter()
        .map(|(name, (read, write))| TopProc {
            name: name.to_owned(),
            read,
            write,
        })
        .collect();
    v.sort_by(|a, b| b.write.cmp(&a.write).then(a.name.cmp(&b.name)));
    v.truncate(n);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(read: u64, write: u64) -> Counts {
        Counts {
            read,
            write,
            ..Counts::default()
        }
    }

    #[test]
    fn minutes_close_when_the_clock_moves_on() {
        let mut acc = MinuteAcc::default();
        for s in 0..3u64 {
            acc.add(
                60_000 + s * 1000,
                &c(10, 100),
                10.0,
                100.0 + s as f64,
                [
                    ("a.exe".to_owned(), c(0, 90)),
                    ("b.exe".to_owned(), c(10, 10)),
                ]
                .into_iter(),
            );
        }
        assert!(acc.take().is_empty(), "minute still open");
        acc.add(120_500, &c(1, 1), 1.0, 1.0, std::iter::empty());
        let done = acc.take();
        assert_eq!(done.len(), 1);
        let m = &done[0];
        assert_eq!((m.ts, m.read, m.write, m.seconds), (60, 30, 300, 3));
        assert_eq!(m.peak_write, 102.0);
        assert_eq!(m.top[0].name, "a.exe");
        assert_eq!(m.top[0].write, 270);
        let open = acc.flush().expect("open minute");
        assert_eq!((open.ts, open.seconds), (120, 1));
        assert!(open.top.is_empty());
    }

    #[test]
    fn top_writers_sum_over_minutes() {
        let m = |ts, name: &str, w| Minute {
            ts,
            read: 0,
            write: w,
            peak_read: 0.0,
            peak_write: 0.0,
            seconds: 60,
            top: vec![TopProc {
                name: name.into(),
                read: 0,
                write: w,
            }],
        };
        let v = top_writers(&[m(0, "a", 5), m(60, "b", 7), m(120, "a", 4)], 5);
        assert_eq!(v[0].name, "a");
        assert_eq!(v[0].write, 9);
        assert_eq!(v.len(), 2);
    }
}
