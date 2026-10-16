//! 24-hour disk activity history for the Monitor (PHASE6_MONITOR §2.3): one row
//! per minute the monitor ran — bytes read / written, peak rates, and the busiest
//! processes as JSON (written by `nfd-monitor`'s downsample, opaque here).
//!
//! Like `file_hashes`, the table is created when missing (no schema version),
//! rows older than 24 hours are dropped on every save, and [`History::clear`]
//! wipes it.

use rusqlite::params;

use crate::{History, Result, to_u64};

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS monitor_minutes (
    ts         INTEGER PRIMARY KEY,
    read       INTEGER NOT NULL,
    write      INTEGER NOT NULL,
    peak_read  REAL    NOT NULL,
    peak_write REAL    NOT NULL,
    seconds    INTEGER NOT NULL,
    top        TEXT    NOT NULL
);
";

/// How long minutes are kept, seconds.
pub const KEEP_SECS: u64 = 24 * 3600;

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorMinute {
    /// Unix time of the minute's start, seconds.
    pub ts: u64,
    pub read: u64,
    pub write: u64,
    pub peak_read: f64,
    pub peak_write: f64,
    /// Seconds sampled in the minute.
    pub seconds: u32,
    /// Busiest processes, JSON.
    pub top: String,
}

impl History {
    /// Save minutes (a minute saved twice — the app restarted within it — adds up)
    /// and drop everything older than 24 hours before `now` (Unix seconds).
    ///
    /// # Errors
    /// SQLite write failure.
    pub fn monitor_save(&mut self, rows: &[MonitorMinute], now: u64) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO monitor_minutes (ts, read, write, peak_read, peak_write, seconds, top)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(ts) DO UPDATE SET
                   read = read + excluded.read,
                   write = write + excluded.write,
                   peak_read = max(peak_read, excluded.peak_read),
                   peak_write = max(peak_write, excluded.peak_write),
                   seconds = min(60, seconds + excluded.seconds),
                   top = CASE WHEN excluded.write >= write THEN excluded.top ELSE top END",
            )?;
            for r in rows {
                stmt.execute(params![
                    r.ts as i64,
                    r.read as i64,
                    r.write as i64,
                    r.peak_read,
                    r.peak_write,
                    i64::from(r.seconds),
                    r.top
                ])?;
            }
            tx.execute(
                "DELETE FROM monitor_minutes WHERE ts < ?1",
                params![now.saturating_sub(KEEP_SECS) as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Minutes from `since` (Unix seconds) on, oldest first.
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn monitor_minutes(&self, since: u64) -> Result<Vec<MonitorMinute>> {
        let mut stmt = self.conn.prepare(
            "SELECT ts, read, write, peak_read, peak_write, seconds, top
             FROM monitor_minutes WHERE ts >= ?1 ORDER BY ts",
        )?;
        let rows = stmt.query_map(params![since as i64], |r| {
            Ok(MonitorMinute {
                ts: to_u64(r.get(0)?),
                read: to_u64(r.get(1)?),
                write: to_u64(r.get(2)?),
                peak_read: r.get(3)?,
                peak_write: r.get(4)?,
                seconds: u32::try_from(r.get::<_, i64>(5)?).unwrap_or(0),
                top: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ts: u64, write: u64) -> MonitorMinute {
        MonitorMinute {
            ts,
            read: 1,
            write,
            peak_read: 1.0,
            peak_write: write as f64,
            seconds: 30,
            top: format!("[{write}]"),
        }
    }

    #[test]
    fn minutes_round_trip_add_up_and_expire() {
        let mut h = History::open_in_memory().expect("db");
        let now = 100_000;
        h.monitor_save(&[row(now - 120, 5), row(now - 60, 7)], now)
            .expect("save");
        h.monitor_save(&[row(now - 60, 9)], now)
            .expect("save again");
        let v = h.monitor_minutes(0).expect("read");
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].write, 16);
        assert_eq!(v[1].read, 2);
        assert_eq!(v[1].seconds, 60);
        assert_eq!(v[1].peak_write, 9.0);
        assert_eq!(v[1].top, "[9]");
        // a day later the old rows are gone
        h.monitor_save(&[row(now + KEEP_SECS, 1)], now + KEEP_SECS)
            .expect("save");
        let v = h.monitor_minutes(0).expect("read");
        assert_eq!(v.len(), 1);
        assert_eq!(
            h.monitor_minutes(now + KEEP_SECS + 1).expect("read").len(),
            0
        );
        h.clear().expect("clear");
        assert!(h.monitor_minutes(0).expect("read").is_empty());
    }
}
