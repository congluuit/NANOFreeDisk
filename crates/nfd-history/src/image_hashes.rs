//! Picture-hash cache for "Similar photos" (`docs/SIMILAR_PHOTOS.md`), next to the
//! content-hash cache of the duplicate finder ([`crate::hashes`]).
//!
//! Same identity and staleness rules as `file_hashes`: keyed by `(volume serial,
//! file id)`, a row is only used while the file still has the size and last write it
//! had when it was decoded, it was decoded at least two seconds after that last
//! write, and it was made by the same hash algorithm (`algo`; a new algorithm simply
//! decodes again). A row also remembers files that could not be compared (`status`
//! other than 0: unreadable, too large, no detail) so a broken picture is not decoded
//! on every run.
//!
//! The table is created when missing (no schema version, older builds ignore it),
//! is capped ([`MAX_ROWS`]) and is wiped with the scan history ([`History::clear`]).

use rusqlite::{OptionalExtension, params};

use crate::hashes::HashKey;
use crate::{History, Result, to_i64, to_u64};

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS image_hashes (
    volume    INTEGER NOT NULL,
    file_id   BLOB    NOT NULL,
    size      INTEGER NOT NULL,
    mtime     INTEGER NOT NULL,
    algo      INTEGER NOT NULL,
    status    INTEGER NOT NULL,
    width     INTEGER NOT NULL,
    height    INTEGER NOT NULL,
    hash      BLOB,
    hashed_at INTEGER NOT NULL,
    PRIMARY KEY (volume, file_id)
) WITHOUT ROWID;
";

/// Rows kept at most; the least recently decoded go first.
pub const MAX_ROWS: u64 = 1_000_000;

/// What is known about one picture.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImageHashRow {
    /// Hash algorithm version of the caller.
    pub algo: u32,
    /// 0 = hashed; other values are the caller's reasons for leaving it out.
    pub status: u8,
    /// Size after the EXIF orientation, in pixels (0 when unknown).
    pub width: u32,
    pub height: u32,
    /// The caller's hash bytes (`None` when `status` is not 0).
    pub hash: Option<Vec<u8>>,
}

impl History {
    /// Cached picture hashes for `keys` made by `algo` (index-aligned; `None` =
    /// unknown, stale or another algorithm).
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn image_hash_lookup(
        &self,
        keys: &[HashKey],
        algo: u32,
    ) -> Result<Vec<Option<ImageHashRow>>> {
        let mut stmt = self.conn.prepare(
            "SELECT size, mtime, algo, status, width, height, hash, hashed_at FROM image_hashes
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
                            r.get::<_, i64>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, i64>(5)?,
                            r.get::<_, Option<Vec<u8>>>(6)?,
                            r.get::<_, i64>(7)?,
                        ))
                    },
                )
                .optional()?;
            out.push(row.and_then(
                |(size, mtime, row_algo, status, width, height, hash, hashed_at)| {
                    let fresh = size == k.size
                        && mtime == k.mtime
                        && hashed_at > k.mtime.saturating_add(1)
                        && row_algo == i64::from(algo);
                    fresh.then(|| ImageHashRow {
                        algo,
                        status: u8::try_from(status).unwrap_or(u8::MAX),
                        width: u32::try_from(width).unwrap_or(0),
                        height: u32::try_from(height).unwrap_or(0),
                        hash,
                    })
                },
            ));
        }
        Ok(out)
    }

    /// Remember picture hashes computed at `now` (Unix seconds).
    ///
    /// # Errors
    /// SQLite write failure.
    pub fn image_hash_store(&mut self, rows: &[(HashKey, ImageHashRow)], now: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO image_hashes
                    (volume, file_id, size, mtime, algo, status, width, height, hash, hashed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for (k, h) in rows {
                stmt.execute(params![
                    k.volume as i64,
                    k.file_id.to_le_bytes().as_slice(),
                    to_i64(k.size),
                    k.mtime,
                    i64::from(h.algo),
                    i64::from(h.status),
                    i64::from(h.width),
                    i64::from(h.height),
                    h.hash.as_deref(),
                    now,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Number of cached picture hashes.
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn image_hash_count(&self) -> Result<u64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM image_hashes", [], |r| r.get(0))?;
        Ok(to_u64(n))
    }

    /// Keep at most `max_rows` picture hashes (the most recently decoded); returns
    /// how many were dropped.
    ///
    /// # Errors
    /// SQLite write failure.
    pub fn image_hash_prune(&mut self, max_rows: u64) -> Result<usize> {
        let n = self.image_hash_count()?;
        if n <= max_rows {
            return Ok(0);
        }
        let dropped = self.conn.execute(
            "DELETE FROM image_hashes WHERE (volume, file_id) IN (
                SELECT volume, file_id FROM image_hashes ORDER BY hashed_at LIMIT ?1)",
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

    fn row(algo: u32) -> ImageHashRow {
        ImageHashRow {
            algo,
            status: 0,
            width: 4000,
            height: 3000,
            hash: Some(vec![7; 80]),
        }
    }

    #[test]
    fn round_trip_staleness_and_algorithm() {
        let mut h = History::open_in_memory().expect("db");
        h.image_hash_store(&[(key(u128::MAX, 10, 100), row(1))], 200)
            .expect("store");
        assert_eq!(
            h.image_hash_lookup(&[key(u128::MAX, 10, 100)], 1)
                .expect("get"),
            [Some(row(1))]
        );
        // another size, date, file or algorithm: unknown
        let other = h
            .image_hash_lookup(
                &[
                    key(u128::MAX, 11, 100),
                    key(u128::MAX, 10, 101),
                    key(7, 10, 100),
                ],
                1,
            )
            .expect("get");
        assert_eq!(other, [None, None, None]);
        assert_eq!(
            h.image_hash_lookup(&[key(u128::MAX, 10, 100)], 2)
                .expect("get"),
            [None]
        );
        // a file that could not be read is remembered too
        let broken = ImageHashRow {
            algo: 1,
            status: 3,
            width: 0,
            height: 0,
            hash: None,
        };
        h.image_hash_store(&[(key(5, 20, 100), broken.clone())], 300)
            .expect("store");
        assert_eq!(
            h.image_hash_lookup(&[key(5, 20, 100)], 1).expect("get"),
            [Some(broken)]
        );
        // decoded in the second it was written: not trusted
        h.image_hash_store(&[(key(6, 20, 1000), row(1))], 1001)
            .expect("store");
        assert_eq!(
            h.image_hash_lookup(&[key(6, 20, 1000)], 1).expect("get"),
            [None]
        );
    }

    #[test]
    fn prune_and_clear() {
        let mut h = History::open_in_memory().expect("db");
        for i in 0..10u8 {
            h.image_hash_store(&[(key(u128::from(i), 1, 0), row(1))], i64::from(i) + 10)
                .expect("store");
        }
        assert_eq!(h.image_hash_prune(4).expect("prune"), 6);
        assert_eq!(h.image_hash_count().expect("count"), 4);
        assert!(h.image_hash_lookup(&[key(9, 1, 0)], 1).expect("get")[0].is_some());
        h.clear().expect("clear");
        assert_eq!(h.image_hash_count().expect("count"), 0);
    }
}
