//! Keeping the journal honest about the Recycle Bin. An item the app moved to the
//! Recycle Bin is "restorable" in the journal for as long as the bin holds it, but
//! the bin can be emptied (from this app's Recycle Bin card, from Explorer, by
//! Storage Sense) without the journal hearing about it. [`History::settle_recycle_bin`]
//! marks such items as deleted for good, so the Activity log stops offering a
//! Restore that would only find nothing.

use std::path::Path;

use rusqlite::params;

use crate::{History, Result, to_i64, to_u64};

/// One Recycle Bin item the journal still calls restorable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinItemRef {
    pub op_id: String,
    pub n: u64,
    /// Where the item was deleted from.
    pub path: String,
    /// When its operation started (Unix seconds): the bin's record of the item
    /// is at least that new.
    pub started_at: i64,
}

impl History {
    /// Recycle Bin items (Windows: no recorded location, the bin's own records
    /// find them) that the journal still calls restorable.
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn recycle_bin_pending(&self) -> Result<Vec<BinItemRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.op_id, i.n, i.path, o.started_at
             FROM operation_items i JOIN operations o ON o.id = i.op_id
             WHERE i.mode = 'recycle-bin' AND i.stored IS NULL AND i.restored = 0
               AND i.purged = 0 AND i.status IN ('done', 'partial') AND o.state != 'purged'
             ORDER BY o.started_at, i.n",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(BinItemRef {
                op_id: r.get(0)?,
                n: to_u64(r.get(1)?),
                path: r.get(2)?,
                started_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Mark the Recycle Bin items that are no longer in the bin as deleted for
    /// good. `in_bin` answers for one item (original path, operation start):
    /// `Some(true)` still there, `Some(false)` gone, `None` unknown (the drive's
    /// bin could not be read: the item keeps its state). An operation whose last
    /// restorable item went becomes `purged`. Returns the number of items marked.
    ///
    /// # Errors
    /// SQLite failure.
    pub fn settle_recycle_bin(
        &mut self,
        in_bin: &dyn Fn(&Path, i64) -> Option<bool>,
    ) -> Result<u64> {
        let gone: Vec<BinItemRef> = self
            .recycle_bin_pending()?
            .into_iter()
            .filter(|i| in_bin(Path::new(&i.path), i.started_at) == Some(false))
            .collect();
        if gone.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        for i in &gone {
            tx.execute(
                "UPDATE operation_items SET purged = 1 WHERE op_id = ?1 AND n = ?2",
                params![i.op_id, to_i64(i.n)],
            )?;
        }
        let mut ops: Vec<&str> = gone.iter().map(|i| i.op_id.as_str()).collect();
        ops.sort_unstable();
        ops.dedup();
        for id in ops {
            // the same test as the journal's own settling after a purge
            tx.execute(
                "UPDATE operations SET state = 'purged' WHERE id = ?1 AND NOT EXISTS (
                     SELECT 1 FROM operation_items WHERE op_id = ?1 AND restored = 0
                       AND purged = 0 AND status IN ('done', 'partial') AND mode != 'permanent')",
                [id],
            )?;
        }
        tx.commit()?;
        Ok(gone.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use nfd_core::exec::{ItemResult, ItemStatus, Operation};
    use nfd_core::plan::DeleteMode;

    use crate::History;

    fn item(n: usize, path: &str, mode: DeleteMode, stored: Option<&str>) -> ItemResult {
        ItemResult {
            n,
            path: PathBuf::from(path),
            rule_id: None,
            mode,
            is_dir: false,
            bytes: 10,
            files: 1,
            freed: 10,
            status: ItemStatus::Done,
            reason: None,
            message: None,
            stored: stored.map(PathBuf::from),
            warnings: Vec::new(),
        }
    }

    fn op(id: &str, items: Vec<ItemResult>) -> Operation {
        Operation {
            id: id.into(),
            kind: "manual".into(),
            started_at: 1_000,
            finished_at: 1_001,
            items,
            cancelled: false,
        }
    }

    #[test]
    fn emptied_bin_items_stop_being_restorable() {
        let mut h = History::open_in_memory().expect("db");
        h.record_operation(&op(
            "20261113-000000-0001",
            vec![
                item(0, r"C:\a.txt", DeleteMode::RecycleBin, None),
                item(1, r"D:\b.txt", DeleteMode::RecycleBin, None),
            ],
        ))
        .expect("record");
        // a quarantined item and a macOS Trash item are never touched
        h.record_operation(&op(
            "20261113-000000-0002",
            vec![
                item(
                    0,
                    r"C:\q.txt",
                    DeleteMode::Quarantine,
                    Some(r"C:\Q\20261113-000000-0002\0\q.txt"),
                ),
                item(
                    1,
                    "/Users/x/c.txt",
                    DeleteMode::RecycleBin,
                    Some("/Users/x/.Trash/c.txt"),
                ),
            ],
        ))
        .expect("record");
        assert_eq!(h.recycle_bin_pending().expect("pending").len(), 2);

        // C: was emptied, D:'s bin could not be read
        let in_bin = |p: &Path, _: i64| -> Option<bool> {
            if p.to_string_lossy().starts_with('C') {
                Some(false)
            } else {
                None
            }
        };
        assert_eq!(h.settle_recycle_bin(&in_bin).expect("settle"), 1);
        let (row, items) = h.operation("20261113-000000-0001").expect("op");
        assert_eq!(row.state, "active");
        assert_eq!(row.restorable, 1);
        assert!(items[0].purged && !items[0].restorable(&row.state));
        assert!(!items[1].purged);

        // then D: too: the whole operation is gone for good
        assert_eq!(
            h.settle_recycle_bin(&|_, _| Some(false)).expect("settle"),
            1
        );
        let (row, _) = h.operation("20261113-000000-0001").expect("op");
        assert_eq!((row.state.as_str(), row.restorable), ("purged", 0));
        let (other, _) = h.operation("20261113-000000-0002").expect("op");
        assert_eq!((other.state.as_str(), other.restorable), ("active", 2));
        assert!(h.recycle_bin_pending().expect("pending").is_empty());
    }

    #[test]
    fn items_still_in_the_bin_keep_their_state() {
        let mut h = History::open_in_memory().expect("db");
        h.record_operation(&op(
            "20261113-000000-0003",
            vec![item(0, r"C:\a.txt", DeleteMode::RecycleBin, None)],
        ))
        .expect("record");
        assert_eq!(h.settle_recycle_bin(&|_, _| Some(true)).expect("settle"), 0);
        assert_eq!(h.operations(5).expect("list")[0].restorable, 1);
    }
}
