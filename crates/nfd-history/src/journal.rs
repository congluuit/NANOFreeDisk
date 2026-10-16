//! The undo journal (PHASE3_CLEAN §2.3): every clean / delete operation with each
//! item's original path, where it went (quarantine folder or Recycle Bin) and its
//! outcome, so the Activity log can list them by day and put items back.
//!
//! Lives in the history database (tables `operations` and `operation_items`, schema
//! 2) but is independent of the scan snapshots: clearing the scan history keeps it.

use std::path::{Path, PathBuf};

use nfd_core::exec::{
    self, ItemStatus, Operation, QUARANTINE_TTL_DAYS, QuarantineRoots, RestoreStatus,
};
use nfd_core::plan::DeleteMode;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::{History, HistoryError, Result, to_i64, to_u64};

pub(crate) const SCHEMA: &str = "
CREATE TABLE operations (
    id          TEXT    PRIMARY KEY,
    kind        TEXT    NOT NULL,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER NOT NULL,
    items       INTEGER NOT NULL,
    done        INTEGER NOT NULL,
    bytes       INTEGER NOT NULL,
    freed       INTEGER NOT NULL,
    cancelled   INTEGER NOT NULL,
    state       TEXT    NOT NULL
);
CREATE INDEX operations_started ON operations (started_at);
CREATE TABLE operation_items (
    op_id    TEXT    NOT NULL REFERENCES operations (id) ON DELETE CASCADE,
    n        INTEGER NOT NULL,
    path     TEXT    NOT NULL,
    rule_id  TEXT,
    mode     TEXT    NOT NULL,
    is_dir   INTEGER NOT NULL,
    bytes    INTEGER NOT NULL,
    files    INTEGER NOT NULL,
    freed    INTEGER NOT NULL,
    status   TEXT    NOT NULL,
    reason   TEXT,
    stored   TEXT,
    restored INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (op_id, n)
) WITHOUT ROWID;
";

/// One operation as the Activity log shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRow {
    pub id: String,
    pub kind: String,
    pub started_at: i64,
    pub finished_at: i64,
    /// Items attempted (selected).
    pub items: u64,
    /// Items that left the disk (done or partial).
    pub done: u64,
    pub bytes: u64,
    pub freed: u64,
    pub cancelled: bool,
    /// `active`, `restored`, `partly-restored` or `purged`.
    pub state: String,
    /// Items that can still be put back.
    pub restorable: u64,
}

/// One item of an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRow {
    pub n: u64,
    pub path: String,
    pub rule_id: Option<String>,
    pub mode: String,
    pub is_dir: bool,
    pub bytes: u64,
    pub files: u64,
    pub freed: u64,
    pub status: String,
    pub reason: Option<String>,
    pub stored: Option<String>,
    pub restored: bool,
}

impl ItemRow {
    fn mode(&self) -> DeleteMode {
        match self.mode.as_str() {
            "quarantine" => DeleteMode::Quarantine,
            "permanent" => DeleteMode::Permanent,
            _ => DeleteMode::RecycleBin,
        }
    }

    /// Can still be put back (as far as the journal knows).
    #[must_use]
    pub fn restorable(&self, state: &str) -> bool {
        !self.restored
            && state != "purged"
            && matches!(self.status.as_str(), "done" | "partial")
            && self.mode != "permanent"
    }
}

/// Result of restoring one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub n: u64,
    pub path: String,
    pub status: RestoreStatus,
    pub message: Option<String>,
}

const OP_COLUMNS: &str =
    "id, kind, started_at, finished_at, items, done, bytes, freed, cancelled, state,
     (SELECT COUNT(*) FROM operation_items i WHERE i.op_id = o.id AND i.restored = 0
        AND i.status IN ('done', 'partial') AND i.mode != 'permanent' AND o.state != 'purged')";

fn op_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<OperationRow> {
    Ok(OperationRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        started_at: r.get(2)?,
        finished_at: r.get(3)?,
        items: to_u64(r.get(4)?),
        done: to_u64(r.get(5)?),
        bytes: to_u64(r.get(6)?),
        freed: to_u64(r.get(7)?),
        cancelled: r.get::<_, i64>(8)? != 0,
        state: r.get(9)?,
        restorable: to_u64(r.get(10)?),
    })
}

impl History {
    /// Store a finished operation. Items that were not selected are left out.
    ///
    /// # Errors
    /// SQLite write failure.
    pub fn record_operation(&mut self, op: &Operation) -> Result<()> {
        let attempted: Vec<_> = op
            .items
            .iter()
            .filter(|i| i.status != ItemStatus::NotSelected)
            .collect();
        let done = attempted
            .iter()
            .filter(|i| i.status.removed_something())
            .count();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO operations
             (id, kind, started_at, finished_at, items, done, bytes, freed, cancelled, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'active')",
            params![
                op.id,
                op.kind,
                op.started_at,
                op.finished_at,
                to_i64(attempted.len() as u64),
                to_i64(done as u64),
                to_i64(attempted.iter().map(|i| i.bytes).sum()),
                to_i64(op.freed()),
                i64::from(op.cancelled),
            ],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO operation_items
                 (op_id, n, path, rule_id, mode, is_dir, bytes, files, freed, status, reason, stored)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for i in attempted {
                stmt.execute(params![
                    op.id,
                    to_i64(i.n as u64),
                    i.path.to_string_lossy(),
                    i.rule_id,
                    i.mode.as_str(),
                    i64::from(i.is_dir),
                    to_i64(i.bytes),
                    to_i64(i.files),
                    to_i64(i.freed),
                    i.status.as_str(),
                    i.reason,
                    i.stored.as_ref().map(|s| s.to_string_lossy().into_owned()),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The newest `limit` operations.
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn operations(&self, limit: usize) -> Result<Vec<OperationRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {OP_COLUMNS} FROM operations o ORDER BY started_at DESC, id DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map([to_i64(limit as u64)], op_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One operation with its items.
    ///
    /// # Errors
    /// Unknown id ([`HistoryError::UnknownOperation`]) or SQLite failure.
    pub fn operation(&self, id: &str) -> Result<(OperationRow, Vec<ItemRow>)> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {OP_COLUMNS} FROM operations o WHERE id = ?1"),
                [id],
                op_row,
            )
            .optional()?
            .ok_or_else(|| HistoryError::UnknownOperation(id.to_owned()))?;
        let mut stmt = self.conn.prepare(
            "SELECT n, path, rule_id, mode, is_dir, bytes, files, freed, status, reason, stored, restored
             FROM operation_items WHERE op_id = ?1 ORDER BY n",
        )?;
        let items = stmt
            .query_map([id], |r| {
                Ok(ItemRow {
                    n: to_u64(r.get(0)?),
                    path: r.get(1)?,
                    rule_id: r.get(2)?,
                    mode: r.get(3)?,
                    is_dir: r.get::<_, i64>(4)? != 0,
                    bytes: to_u64(r.get(5)?),
                    files: to_u64(r.get(6)?),
                    freed: to_u64(r.get(7)?),
                    status: r.get(8)?,
                    reason: r.get(9)?,
                    stored: r.get(10)?,
                    restored: r.get::<_, i64>(11)? != 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((row, items))
    }

    /// Put back the items of operation `id` (all restorable ones, or only `only`).
    /// Items restored are marked; the operation becomes `restored` when nothing
    /// restorable is left, else `partly-restored`.
    ///
    /// # Errors
    /// Unknown id or SQLite failure (file-system failures are per item).
    pub fn restore_operation(
        &mut self,
        id: &str,
        only: Option<&[u64]>,
    ) -> Result<Vec<RestoreResult>> {
        let (row, items) = self.operation(id)?;
        let mut out = Vec::new();
        for item in items
            .iter()
            .filter(|i| i.restorable(&row.state))
            .filter(|i| only.is_none_or(|o| o.contains(&i.n)))
        {
            let (status, message) = exec::restore(
                Path::new(&item.path),
                item.stored.as_deref().map(Path::new),
                item.mode(),
                row.started_at,
            );
            if matches!(status, RestoreStatus::Restored | RestoreStatus::Missing) {
                self.conn.execute(
                    "UPDATE operation_items SET restored = 1 WHERE op_id = ?1 AND n = ?2",
                    params![id, to_i64(item.n)],
                )?;
            }
            out.push(RestoreResult {
                n: item.n,
                path: item.path.clone(),
                status,
                message,
            });
        }
        let left: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM operation_items WHERE op_id = ?1 AND restored = 0
             AND status IN ('done', 'partial') AND mode != 'permanent'",
            [id],
            |r| r.get(0),
        )?;
        let any_restored: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM operation_items WHERE op_id = ?1 AND restored = 1",
            [id],
            |r| r.get(0),
        )?;
        if any_restored > 0 {
            let state = if left == 0 {
                "restored"
            } else {
                "partly-restored"
            };
            if left == 0 {
                // only the manifest and empty slots are left
                for dir in op_dirs(&items, id) {
                    let _ = exec::purge_op_dir(&dir);
                }
            }
            self.conn.execute(
                "UPDATE operations SET state = ?2 WHERE id = ?1",
                params![id, state],
            )?;
        }
        Ok(out)
    }

    /// Delete the quarantine folders of operation `id` for good and mark it `purged`.
    /// Recycle Bin items are left to Windows.
    ///
    /// # Errors
    /// SQLite failure; folders that cannot be removed are reported as an I/O error
    /// after the others were removed.
    pub fn purge_operation(&mut self, id: &str) -> Result<()> {
        let (_, items) = self.operation(id)?;
        let mut first_err = None;
        for dir in op_dirs(&items, id) {
            if let Err(e) = exec::purge_op_dir(&dir) {
                first_err.get_or_insert(e);
            }
        }
        self.conn
            .execute("UPDATE operations SET state = 'purged' WHERE id = ?1", [id])?;
        match first_err {
            Some(e) => Err(e.into()),
            None => Ok(()),
        }
    }

    /// Purge every operation whose quarantine is older than `ttl_days` (default
    /// [`QUARANTINE_TTL_DAYS`]), plus leftover operation folders in `roots` the journal
    /// does not know about (a cleared or damaged database). Returns the ids purged.
    ///
    /// # Errors
    /// SQLite failure.
    pub fn purge_expired(
        &mut self,
        roots: &QuarantineRoots,
        now: i64,
        ttl_days: Option<i64>,
    ) -> Result<Vec<String>> {
        let cutoff = now - ttl_days.unwrap_or(QUARANTINE_TTL_DAYS) * 86_400;
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT DISTINCT o.id FROM operations o JOIN operation_items i ON i.op_id = o.id
                 WHERE o.state != 'purged' AND o.started_at < ?1 AND i.mode = 'quarantine'
                   AND i.restored = 0 AND i.stored IS NOT NULL",
            )?;
            stmt.query_map([cutoff], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut purged = Vec::new();
        for id in ids {
            if self.purge_operation(&id).is_ok() {
                purged.push(id);
            }
        }
        // leftovers: folder name `YYYYMMDD-…` older than the cutoff
        let (y, m, d, ..) = exec::civil(cutoff);
        let cutoff_day = format!("{y:04}{m:02}{d:02}");
        for root in roots.existing() {
            let Ok(rd) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let is_op = name.len() == 20
                    && name.as_bytes()[8] == b'-'
                    && name[..8].bytes().all(|b| b.is_ascii_digit());
                if !is_op || name[..8] >= *cutoff_day {
                    continue;
                }
                let known: bool = self
                    .conn
                    .query_row(
                        "SELECT state != 'purged' FROM operations WHERE id = ?1",
                        [&name],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or(false);
                if !known && exec::purge_op_dir(&entry.path()).is_ok() {
                    purged.push(name);
                }
            }
        }
        Ok(purged)
    }

    /// Forget operations older than `days` days (the journal keeps 90 days like the
    /// audit log). Their quarantine must already be purged.
    ///
    /// # Errors
    /// SQLite failure.
    pub fn prune_operations(&mut self, now: i64, days: i64) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM operations WHERE started_at < ?1 AND (state IN ('purged', 'restored')
             OR NOT EXISTS (SELECT 1 FROM operation_items i WHERE i.op_id = operations.id
                 AND i.mode = 'quarantine' AND i.restored = 0 AND i.stored IS NOT NULL))",
            [now - days * 86_400],
        )?;
        Ok(n)
    }
}

/// The `<root>\<op id>` folders that hold an operation's quarantined items.
fn op_dirs(items: &[ItemRow], id: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = items
        .iter()
        .filter_map(|i| i.stored.as_deref())
        .filter_map(|s| {
            Path::new(s)
                .ancestors()
                .find(|a| a.file_name().is_some_and(|n| n == id))
                .map(Path::to_path_buf)
        })
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}
