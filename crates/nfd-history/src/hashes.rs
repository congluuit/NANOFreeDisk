//! Content-hash cache for the duplicate finder (PHASE5_DUPLICATES §2.1).
//!
//! Keyed by `(volume serial, file id)`; a row is only used while the file still has
//! the size and last-write time it had when it was hashed **and** it was hashed at
//! least two seconds after that last write (last-write times have one-second
//! resolution here: a file rewritten in the same second it was hashed would
//! otherwise keep a stale hash). Anything different means "hash again".
//!
//! The table lives in the history database but is not part of a schema version: it
//! is created when missing, so older builds simply ignore it. It is capped
//! ([`MAX_ROWS`]) and wiped together with the scan history ([`History::clear`]).

use rusqlite::{OptionalExtension, params};

use crate::{History, Result, to_i64, to_u64};

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS file_hashes (
    volume    INTEGER NOT NULL,
    file_id   BLOB    NOT NULL,
    size      INTEGER NOT NULL,
    mtime     INTEGER NOT NULL,
    partial   BLOB,
    full      BLOB,
    hashed_at INTEGER NOT NULL,
    PRIMARY KEY (volume, file_id)
) WITHOUT ROWID;
";

/// Rows kept at most; the least recently hashed go first.
pub const MAX_ROWS: u64 = 2_000_000;

/// Identity of one file's contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HashKey {
    pub volume: u64,
    pub file_id: u128,
    pub size: u64,
    /// Last write, Unix seconds.
    pub mtime: i64,
}

/// What is known about a file's contents (BLAKE3 digests).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HashRow {
    /// Head + middle + tail sample.
    pub partial: Option<[u8; 32]>,
    /// The whole file.
    pub full: Option<[u8; 32]>,
}

fn digest(blob: Option<Vec<u8>>) -> Option<[u8; 32]> {
    blob.and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
}

impl History {
    /// Cached hashes for `keys` (index-aligned; `None` = unknown or stale).
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn hash_lookup(&self, keys: &[HashKey]) -> Result<Vec<Option<HashRow>>> {
        let mut stmt = self.conn.prepare(
            "SELECT size, mtime, partial, full, hashed_at FROM file_hashes
             WHERE volume = ?1 AND file_id = ?2",
        )?;
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            let row = stmt
                .query_row(
                    params![k.volume as i64, k.file_id.to_le_bytes().as_slice()],
                    |r| {
                        Ok((
                            to_u64(r.get(0)?),
                            r.get::<_, i64>(1)?,
                            r.get::<_, Option<Vec<u8>>>(2)?,
                            r.get::<_, Option<Vec<u8>>>(3)?,
                            r.get::<_, i64>(4)?,
                        ))
                    },
                )
                .optional()?;
            out.push(row.and_then(|(size, mtime, partial, full, hashed_at)| {
                (size == k.size && mtime == k.mtime && hashed_at > k.mtime.saturating_add(1)).then(
                    || HashRow {
                        partial: digest(partial),
                        full: digest(full),
                    },
                )
            }));
        }
        Ok(out)
    }

    /// Remember hashes computed at `now` (Unix seconds). A row without a full hash
    /// keeps the full hash already stored for the same size and last write.
    ///
    /// # Errors
    /// SQLite write failure.
    pub fn hash_store(&mut self, rows: &[(HashKey, HashRow)], now: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO file_hashes (volume, file_id, size, mtime, partial, full, hashed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (volume, file_id) DO UPDATE SET
                    partial   = COALESCE(excluded.partial, CASE WHEN size = excluded.size
                                  AND mtime = excluded.mtime THEN partial END),
                    full      = COALESCE(excluded.full, CASE WHEN size = excluded.size
                                  AND mtime = excluded.mtime THEN full END),
                    size      = excluded.size,
                    mtime     = excluded.mtime,
                    hashed_at = excluded.hashed_at",
            )?;
            for (k, h) in rows {
                stmt.execute(params![
                    k.volume as i64,
                    k.file_id.to_le_bytes().as_slice(),
                    to_i64(k.size),
                    k.mtime,
                    h.partial.as_ref().map(<[u8; 32]>::as_slice),
                    h.full.as_ref().map(<[u8; 32]>::as_slice),
                    now,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Number of cached hashes.
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn hash_count(&self) -> Result<u64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM file_hashes", [], |r| r.get(0))?;
        Ok(to_u64(n))
    }

    /// Keep at most `max_rows` cached hashes (the most recently hashed); returns how
    /// many were dropped.
    ///
    /// # Errors
    /// SQLite write failure.
    pub fn hash_prune(&mut self, max_rows: u64) -> Result<usize> {
        let n = self.hash_count()?;
        if n <= max_rows {
            return Ok(0);
        }
        let dropped = self.conn.execute(
            "DELETE FROM file_hashes WHERE (volume, file_id) IN (
                SELECT volume, file_id FROM file_hashes ORDER BY hashed_at LIMIT ?1)",
            [to_i64(n - max_rows)],
        )?;
        Ok(dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: u128, size: u64, mtime: i64) -> HashKey {
        HashKey {
            volume: 0xdead_beef_cafe_f00d,
            file_id: id,
            size,
            mtime,
        }
    }

    #[test]
    fn round_trip_and_staleness() {
        let mut h = History::open_in_memory().expect("db");
        let full = HashRow {
            partial: Some([1; 32]),
            full: Some([2; 32]),
        };
        h.hash_store(&[(key(u128::MAX, 10, 100), full)], 200)
            .expect("store");
        assert_eq!(
            h.hash_lookup(&[key(u128::MAX, 10, 100)]).expect("get"),
            [Some(full)]
        );
        // another size, another date, another file: unknown
        let other = h
            .hash_lookup(&[
                key(u128::MAX, 11, 100),
                key(u128::MAX, 10, 101),
                key(7, 10, 100),
            ])
            .expect("get");
        assert_eq!(other, [None, None, None]);

        // a partial-only update for the same contents keeps the full hash
        h.hash_store(
            &[(
                key(u128::MAX, 10, 100),
                HashRow {
                    partial: Some([1; 32]),
                    full: None,
                },
            )],
            300,
        )
        .expect("store");
        assert_eq!(
            h.hash_lookup(&[key(u128::MAX, 10, 100)]).expect("get")[0],
            Some(full)
        );
        // new contents drop the old full hash
        h.hash_store(
            &[(
                key(u128::MAX, 12, 400),
                HashRow {
                    partial: Some([3; 32]),
                    full: None,
                },
            )],
            500,
        )
        .expect("store");
        assert_eq!(
            h.hash_lookup(&[key(u128::MAX, 12, 400)]).expect("get")[0],
            Some(HashRow {
                partial: Some([3; 32]),
                full: None
            })
        );
    }

    #[test]
    fn hashed_in_the_same_second_as_written_is_not_trusted() {
        let mut h = History::open_in_memory().expect("db");
        let row = HashRow {
            partial: Some([1; 32]),
            full: Some([1; 32]),
        };
        h.hash_store(&[(key(1, 5, 1000), row), (key(2, 5, 1000), row)], 1001)
            .expect("store");
        h.hash_store(&[(key(2, 5, 1000), row)], 1002)
            .expect("store");
        let got = h
            .hash_lookup(&[key(1, 5, 1000), key(2, 5, 1000)])
            .expect("get");
        assert_eq!(got, [None, Some(row)]);
    }

    #[test]
    fn prune_and_clear() {
        let mut h = History::open_in_memory().expect("db");
        let row = HashRow::default();
        let rows: Vec<_> = (0..10).map(|i| (key(i, 1, 0), row)).collect();
        for (n, r) in rows.iter().enumerate() {
            h.hash_store(std::slice::from_ref(r), n as i64 + 10)
                .expect("store");
        }
        assert_eq!(h.hash_prune(4).expect("prune"), 6);
        assert_eq!(h.hash_count().expect("count"), 4);
        // the newest stay
        assert!(h.hash_lookup(&[key(9, 1, 0)]).expect("get")[0].is_some());
        h.clear().expect("clear");
        assert_eq!(h.hash_count().expect("count"), 0);
    }
}
