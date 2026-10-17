//! Scan history (PLAN §12 "lịch sử quét"): every finished scan can be stored as a
//! compact snapshot in a local SQLite file, which powers
//!
//! * **"What changed?"** (A1) — [`History::diff`] compares two snapshots of the same
//!   folder: which folders grew or shrank, which large files appeared or went away,
//!   and how the drive's free space moved;
//! * the **free-space forecast** (F9) used by the Health Score — [`History::forecast`]
//!   fits a line through the free space recorded for one volume.
//!
//! A snapshot is deliberately small: folder sizes only for folders of at least
//! [`RecordOptions::min_dir_bytes`] (plus the first [`RecordOptions::always_depth`]
//! levels), and only large files, so a million-file drive costs a few tens of
//! thousands of rows. Each snapshot remembers the size above which its lists are
//! complete (`dir_floor`, `file_floor`) so a diff never claims something disappeared
//! when it simply was not recorded.
//!
//! The database is local only, holds paths and sizes (no file contents), keeps
//! [`RecordOptions::keep`] snapshots per folder and can be wiped with
//! [`History::clear`]. A damaged file is set aside and replaced instead of breaking
//! the app.
//!
//! The same file holds the [`journal`] of clean/delete operations (Activity log,
//! restore, quarantine purge), and [`audit`] writes a plain-text copy of it, and the
//! duplicate finder's content-hash cache ([`hashes`]).

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Duration;

use nfd_core::model::{EntryId, EntryKind};
use nfd_core::scanner::ScanTree;
use rusqlite::{Connection, params};
use serde::Serialize;
use thiserror::Error;

pub mod audit;
pub mod hashes;
pub mod journal;
pub mod monitor;
pub mod uninstall;

pub use hashes::{HashKey, HashRow};
pub use journal::{ItemRow, OperationRow, RestoreResult};

/// Application identifier: the desktop app keeps its data in a folder of this name.
pub const APP_ID: &str = "vn.io.nanodev.nanofreedisk";

/// Marker file next to the executables of the portable zip.
pub const PORTABLE_FLAG: &str = "portable.flag";

/// Portable mode: when [`PORTABLE_FLAG`] sits next to the running executable, every
/// piece of data (history, settings, logs, quarantine of the system drive, WebView2
/// profile) lives in `data` beside it instead of the user profile. Shared by the app
/// and `nfd.exe`, which ship side by side.
#[must_use]
pub fn portable_dir() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    portable_dir_for(&exe)
}

/// [`portable_dir`] for an executable at `exe`.
#[must_use]
pub fn portable_dir_for(exe: &Path) -> Option<std::path::PathBuf> {
    let dir = exe.parent()?;
    dir.join(PORTABLE_FLAG).is_file().then(|| dir.join("data"))
}

/// Where the desktop app keeps the history (`%LOCALAPPDATA%\<APP_ID>\history.db` on
/// Windows, the XDG data folder elsewhere; `data\history.db` next to the exe in
/// portable mode), so the CLI reads the same file.
#[must_use]
pub fn default_path() -> Option<std::path::PathBuf> {
    if let Some(dir) = portable_dir() {
        return Some(dir.join("history.db"));
    }
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
    };
    let base = if cfg!(windows) {
        env("LOCALAPPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library").join("Application Support")
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local").join("share")))?
    };
    Some(base.join(APP_ID).join("history.db"))
}

/// `PRAGMA user_version` written by this version.
pub const SCHEMA_VERSION: i64 = 2;

const SCHEMA: &str = "
CREATE TABLE scans (
    id           INTEGER PRIMARY KEY,
    root         TEXT    NOT NULL,
    root_key     TEXT    NOT NULL,
    volume       TEXT    NOT NULL,
    started_at   INTEGER NOT NULL,
    elapsed_ms   INTEGER NOT NULL,
    files        INTEGER NOT NULL,
    dirs         INTEGER NOT NULL,
    on_disk      INTEGER NOT NULL,
    volume_total INTEGER NOT NULL,
    volume_free  INTEGER NOT NULL,
    issues       INTEGER NOT NULL,
    dir_floor    INTEGER NOT NULL,
    file_floor   INTEGER NOT NULL
);
CREATE INDEX scans_root ON scans (root_key, started_at);
CREATE INDEX scans_volume ON scans (volume, started_at);
CREATE TABLE dirs (
    scan_id INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    key     TEXT    NOT NULL,
    path    TEXT    NOT NULL,
    depth   INTEGER NOT NULL,
    on_disk INTEGER NOT NULL,
    files   INTEGER NOT NULL,
    PRIMARY KEY (scan_id, key)
) WITHOUT ROWID;
CREATE TABLE files (
    scan_id INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    key     TEXT    NOT NULL,
    path    TEXT    NOT NULL,
    on_disk INTEGER NOT NULL,
    mtime   INTEGER NOT NULL,
    PRIMARY KEY (scan_id, key)
) WITHOUT ROWID;
";

/// History failure.
#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("history database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("history file: {0}")]
    Io(#[from] std::io::Error),
    #[error("history database is damaged ({0})")]
    Damaged(String),
    #[error("history database was written by a newer NANOFreeDisk (schema {0})")]
    Newer(i64),
    #[error("unknown snapshot {0}")]
    NotFound(i64),
    #[error("unknown operation {0}")]
    UnknownOperation(String),
}

pub type Result<T> = std::result::Result<T, HistoryError>;

/// What a snapshot keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordOptions {
    /// Snapshots kept per scanned folder (older ones are deleted).
    pub keep: usize,
    /// Snapshots kept in all (the oldest go first), so scanning many different
    /// folders cannot grow the file without bound.
    pub keep_total: usize,
    /// Folders at least this big are recorded at any depth.
    pub min_dir_bytes: u64,
    /// Folders up to this depth below the root are always recorded.
    pub always_depth: u8,
    /// Cap on recorded folders (the largest win).
    pub max_dirs: usize,
    /// Files at least this big are recorded …
    pub min_file_bytes: u64,
    /// … plus the largest files whatever their size …
    pub top_files: usize,
    /// … up to this many in total (the largest win).
    pub max_files: usize,
}

impl Default for RecordOptions {
    fn default() -> Self {
        Self {
            keep: 30,
            keep_total: 300,
            min_dir_bytes: 1 << 20,
            always_depth: 2,
            max_dirs: 40_000,
            min_file_bytes: 10 << 20,
            top_files: 500,
            max_files: 5_000,
        }
    }
}

/// Size and free space of the volume at scan time (0 = unknown).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VolumeState {
    pub total: u64,
    pub free: u64,
}

/// One stored scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: i64,
    pub root: String,
    /// Volume root the scan lives on, lower-case (`c:\`), empty when unknown.
    pub volume: String,
    /// Unix seconds.
    pub started_at: i64,
    pub elapsed_ms: u64,
    pub files: u64,
    pub dirs: u64,
    pub on_disk: u64,
    pub volume_total: u64,
    pub volume_free: u64,
    pub issues: u64,
    /// Every folder at least this big was recorded.
    pub dir_floor: u64,
    /// Every file at least this big was recorded.
    pub file_floor: u64,
}

/// A folder that grew or shrank between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirChange {
    /// Path relative to the scanned folder.
    pub path: String,
    pub depth: u32,
    /// 0 when the folder was not recorded (new, or smaller than the older floor).
    pub before: u64,
    /// 0 when the folder is gone (or smaller than the newer floor).
    pub after: u64,
    pub delta: i64,
    pub files_delta: i64,
}

/// A large file that appeared, grew, shrank or went away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub before: u64,
    pub after: u64,
    pub delta: i64,
    pub mtime: i64,
}

/// Result of [`History::diff`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub older: Snapshot,
    pub newer: Snapshot,
    pub on_disk_delta: i64,
    pub files_delta: i64,
    /// Change of the volume's free space (negative = less free), when both
    /// snapshots know it and live on the same volume. Includes changes outside the
    /// scanned folder.
    pub free_delta: Option<i64>,
    /// Folders that grew, biggest growth first (most specific folder only).
    pub grew: Vec<DirChange>,
    /// Folders that shrank, biggest drop first.
    pub shrank: Vec<DirChange>,
    /// Large files that appeared or grew, biggest growth first.
    pub files_up: Vec<FileChange>,
    /// Large files that went away or shrank, biggest drop first.
    pub files_down: Vec<FileChange>,
}

/// Knobs of [`History::diff`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffOptions {
    /// Changes smaller than this are ignored.
    pub min_delta: u64,
    /// Rows per list.
    pub limit: usize,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            min_delta: 10 << 20,
            limit: 100,
        }
    }
}

/// Free-space trend of one volume (F9).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Forecast {
    pub volume: String,
    pub free: u64,
    pub total: u64,
    /// Fitted change of free space per day (negative = the drive is filling).
    pub bytes_per_day: f64,
    /// Days until no free space is left at that rate; `None` when not filling.
    pub days_to_full: Option<f64>,
    pub samples: usize,
    pub span_days: f64,
}

/// Lower-case key used to match the same folder across scans.
#[must_use]
pub fn root_key(root: &str) -> String {
    let lower = root.to_lowercase();
    let trimmed = lower.trim_end_matches(['\\', '/']);
    // keep "c:\" (a bare "c:" means the current directory of drive C)
    if trimmed.len() <= 2 && trimmed.ends_with(':') {
        format!("{trimmed}\\")
    } else {
        trimmed.to_owned()
    }
}

fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn to_u64(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

/// Keep the `cap` largest of `items`; returns the size just above the largest one
/// dropped (everything at least that big is kept), or 0 when nothing was dropped.
fn keep_largest(items: &mut Vec<(u64, EntryId)>, cap: usize) -> u64 {
    if items.len() <= cap {
        return 0;
    }
    items.select_nth_unstable_by_key(cap, |&(size, id)| (Reverse(size), id));
    let floor = items[cap..].iter().map(|&(s, _)| s).max().unwrap_or(0);
    items.truncate(cap);
    floor.saturating_add(1)
}

/// The SQLite-backed snapshot store.
#[derive(Debug)]
pub struct History {
    conn: Connection,
}

impl History {
    /// Open (or create) the database at `path`. A damaged file is renamed to
    /// `*.broken-<unix time>` and a fresh one is created; a file from a newer
    /// version is left alone and reported as [`HistoryError::Newer`].
    ///
    /// # Errors
    /// The folder cannot be created, or SQLite fails on a fresh file too.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        match Connection::open(path)
            .map_err(HistoryError::from)
            .and_then(Self::init)
        {
            Ok(h) => Ok(h),
            Err(HistoryError::Sqlite(_) | HistoryError::Damaged(_)) => {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                let mut aside = path.as_os_str().to_owned();
                aside.push(format!(".broken-{stamp}"));
                fs::rename(path, &aside)?;
                for ext in ["-wal", "-shm"] {
                    let mut side = path.as_os_str().to_owned();
                    side.push(ext);
                    let _ = fs::remove_file(side);
                }
                Self::init(Connection::open(path)?)
            }
            Err(e) => Err(e),
        }
    }

    /// A throw-away database (tests, "history off").
    ///
    /// # Errors
    /// SQLite cannot allocate the database.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(HistoryError::Damaged(check));
        }
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(HistoryError::Newer(version));
        }
        // WAL keeps readers (the UI) unblocked while a snapshot is written; an
        // in-memory database answers "memory", which is fine.
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")?;
        if version == 0 {
            conn.execute_batch(&format!(
                "BEGIN; {SCHEMA} {} PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;",
                journal::SCHEMA
            ))?;
        } else if version == 1 {
            // 1 → 2: the undo journal
            conn.execute_batch(&format!(
                "BEGIN; {} PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;",
                journal::SCHEMA
            ))?;
        }
        journal::add_message_column(&conn)?;
        conn.execute_batch(hashes::SCHEMA)?;
        conn.execute_batch(monitor::SCHEMA)?;
        Ok(Self { conn })
    }

    /// Store a snapshot of `tree` (scanned on `volume`, e.g. `C:\`) and drop the
    /// oldest snapshots of the same folder beyond [`RecordOptions::keep`]. Returns
    /// the new snapshot id.
    ///
    /// # Errors
    /// SQLite write failure (disk full, read-only profile…).
    pub fn record(
        &mut self,
        tree: &ScanTree,
        volume: &str,
        state: VolumeState,
        o: &RecordOptions,
    ) -> Result<i64> {
        let n = tree.entries.len();
        let mut depth = vec![0u32; n];
        let mut dirs: Vec<(u64, EntryId)> = Vec::new();
        let mut files: Vec<(u64, EntryId)> = Vec::new();
        for e in &tree.entries {
            let i = e.id.index();
            if let Some(p) = e.parent {
                depth[i] = depth[p.index()] + 1;
            }
            if e.removed {
                continue;
            }
            match e.kind {
                EntryKind::Dir
                    if depth[i] <= u32::from(o.always_depth) || e.on_disk >= o.min_dir_bytes =>
                {
                    dirs.push((e.on_disk, e.id));
                }
                EntryKind::File if !e.hardlink_dup && e.on_disk >= o.min_file_bytes => {
                    files.push((e.on_disk, e.id));
                }
                _ => {}
            }
        }
        let dir_floor = keep_largest(&mut dirs, o.max_dirs).max(o.min_dir_bytes);
        let mut seen: HashSet<EntryId> = files.iter().map(|&(_, id)| id).collect();
        for id in tree.top_files(o.top_files, 0) {
            if seen.insert(id) {
                files.push((tree.entry(id).on_disk, id));
            }
        }
        let file_floor = keep_largest(&mut files, o.max_files).max(o.min_file_bytes);

        let rel = |id: EntryId| -> String {
            if id.index() == 0 {
                return String::new();
            }
            let full = tree.path(id);
            full.strip_prefix(&tree.root)
                .unwrap_or(&full)
                .to_string_lossy()
                .into_owned()
        };
        let root = tree.root.to_string_lossy().into_owned();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO scans (root, root_key, volume, started_at, elapsed_ms, files, dirs,
                 on_disk, volume_total, volume_free, issues, dir_floor, file_floor)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                root,
                root_key(&root),
                volume.to_lowercase(),
                tree.started_at,
                to_i64(u64::try_from(tree.elapsed.as_millis()).unwrap_or(u64::MAX)),
                to_i64(tree.stats.files),
                to_i64(tree.stats.dirs),
                to_i64(tree.stats.on_disk),
                to_i64(state.total),
                to_i64(state.free),
                to_i64(tree.issues.len() as u64),
                to_i64(dir_floor),
                to_i64(file_floor),
            ],
        )?;
        let scan = tx.last_insert_rowid();
        {
            let mut ins = tx.prepare(
                "INSERT OR IGNORE INTO dirs (scan_id, key, path, depth, on_disk, files)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for &(size, id) in &dirs {
                let path = rel(id);
                ins.execute(params![
                    scan,
                    path.to_lowercase(),
                    path,
                    depth[id.index()],
                    to_i64(size),
                    to_i64(tree.file_counts[id.index()]),
                ])?;
            }
            let mut ins = tx.prepare(
                "INSERT OR IGNORE INTO files (scan_id, key, path, on_disk, mtime)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for &(size, id) in &files {
                let path = rel(id);
                ins.execute(params![
                    scan,
                    path.to_lowercase(),
                    path,
                    to_i64(size),
                    tree.entry(id).mtime
                ])?;
            }
        }
        tx.execute(
            "DELETE FROM scans WHERE root_key = ?1 AND id NOT IN (
                 SELECT id FROM scans WHERE root_key = ?1
                 ORDER BY started_at DESC, id DESC LIMIT ?2)",
            params![root_key(&root), to_i64(o.keep.max(1) as u64)],
        )?;
        tx.execute(
            "DELETE FROM scans WHERE id NOT IN (
                 SELECT id FROM scans ORDER BY started_at DESC, id DESC LIMIT ?1)",
            [to_i64(o.keep_total.max(1) as u64)],
        )?;
        tx.commit()?;
        Ok(scan)
    }

    /// Delete every snapshot of `root`; returns how many were removed.
    ///
    /// # Errors
    /// SQLite write failure.
    pub fn forget(&mut self, root: &str) -> Result<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM scans WHERE root_key = ?1", [root_key(root)])?)
    }

    fn snapshot_rows(&self, sql: &str, arg: &dyn rusqlite::ToSql) -> Result<Vec<Snapshot>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([arg], |r| {
            Ok(Snapshot {
                id: r.get(0)?,
                root: r.get(1)?,
                volume: r.get(2)?,
                started_at: r.get(3)?,
                elapsed_ms: to_u64(r.get(4)?),
                files: to_u64(r.get(5)?),
                dirs: to_u64(r.get(6)?),
                on_disk: to_u64(r.get(7)?),
                volume_total: to_u64(r.get(8)?),
                volume_free: to_u64(r.get(9)?),
                issues: to_u64(r.get(10)?),
                dir_floor: to_u64(r.get(11)?),
                file_floor: to_u64(r.get(12)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    const COLUMNS: &str = "id, root, volume, started_at, elapsed_ms, files, dirs, on_disk,
        volume_total, volume_free, issues, dir_floor, file_floor";

    /// Snapshots of `root`, newest first.
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn snapshots(&self, root: &str) -> Result<Vec<Snapshot>> {
        self.snapshot_rows(
            &format!(
                "SELECT {} FROM scans WHERE root_key = ?1 ORDER BY started_at DESC, id DESC",
                Self::COLUMNS
            ),
            &root_key(root),
        )
    }

    /// One snapshot by id.
    ///
    /// # Errors
    /// [`HistoryError::NotFound`] or a SQLite read failure.
    pub fn snapshot(&self, id: i64) -> Result<Snapshot> {
        self.snapshot_rows(
            &format!("SELECT {} FROM scans WHERE id = ?1", Self::COLUMNS),
            &id,
        )?
        .pop()
        .ok_or(HistoryError::NotFound(id))
    }

    /// Number of stored snapshots (all folders).
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn count(&self) -> Result<u64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM scans", [], |r| r.get(0))?;
        Ok(to_u64(n))
    }

    /// Delete every snapshot and the duplicate finder's hash cache, and shrink the
    /// file. The undo journal is kept.
    ///
    /// # Errors
    /// SQLite write failure.
    pub fn clear(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM scans; DELETE FROM file_hashes; DELETE FROM monitor_minutes; VACUUM;",
        )?;
        Ok(())
    }

    fn sizes(&self, table: &str, scan: i64) -> Result<HashMap<String, (String, u64, i64, i64)>> {
        // dirs: (path, on_disk, depth, files); files: (path, on_disk, mtime, 0)
        let sql = if table == "dirs" {
            "SELECT key, path, on_disk, depth, files FROM dirs WHERE scan_id = ?1"
        } else {
            "SELECT key, path, on_disk, mtime, 0 FROM files WHERE scan_id = ?1"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([scan], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (
                    r.get::<_, String>(1)?,
                    to_u64(r.get(2)?),
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ),
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Compare two snapshots (normally of the same folder; `older` is the earlier).
    ///
    /// Folder changes are reported at the most specific level: a folder whose
    /// change is mostly (≥ 80 %) explained by one of its sub-folders is left out in
    /// favour of that sub-folder.
    ///
    /// # Errors
    /// [`HistoryError::NotFound`] or a SQLite read failure.
    pub fn diff(&self, older: i64, newer: i64, o: &DiffOptions) -> Result<Diff> {
        let (a, b) = (self.snapshot(older)?, self.snapshot(newer)?);
        let (da, db) = (self.sizes("dirs", a.id)?, self.sizes("dirs", b.id)?);
        let min = i64::try_from(o.min_delta).unwrap_or(i64::MAX);

        let mut changes: Vec<(String, DirChange)> = Vec::new();
        let keys: HashSet<&String> = da.keys().chain(db.keys()).collect();
        for key in keys {
            if key.is_empty() {
                continue; // the scanned folder itself: see on_disk_delta
            }
            let before = da.get(key);
            let after = db.get(key);
            let (path, depth) = after
                .or(before)
                .map(|x| (x.0.clone(), x.2))
                .unwrap_or_default();
            let (bs, bf) = before.map_or((0, 0), |x| (x.1, x.3));
            let (as_, af) = after.map_or((0, 0), |x| (x.1, x.3));
            let delta = to_i64(as_) - to_i64(bs);
            if delta.abs() < min {
                continue;
            }
            changes.push((
                key.clone(),
                DirChange {
                    path,
                    depth: u32::try_from(depth).unwrap_or(0),
                    before: bs,
                    after: as_,
                    delta,
                    files_delta: af - bf,
                },
            ));
        }
        // drop a folder when one child explains most of its change
        // per parent: (largest growth, largest drop) among its changed children
        let mut best_child: HashMap<&str, (i64, i64)> = HashMap::new();
        for (key, c) in &changes {
            let parent = key.rfind(['\\', '/']).map_or("", |i| &key[..i]);
            let slot = best_child.entry(parent).or_insert((0, 0));
            slot.0 = slot.0.max(c.delta);
            slot.1 = slot.1.min(c.delta);
        }
        let specific = |key: &str, c: &DirChange| {
            let child = best_child
                .get(key)
                .map_or(0, |&(up, down)| if c.delta > 0 { up } else { down });
            i128::from(child.abs()) * 5 < i128::from(c.delta.abs()) * 4
        };
        let mut grew: Vec<DirChange> = Vec::new();
        let mut shrank: Vec<DirChange> = Vec::new();
        for (key, c) in &changes {
            if !specific(key, c) {
                continue;
            }
            if c.delta > 0 {
                grew.push(c.clone());
            } else {
                shrank.push(c.clone());
            }
        }
        grew.sort_by(|x, y| y.delta.cmp(&x.delta).then_with(|| x.path.cmp(&y.path)));
        shrank.sort_by(|x, y| x.delta.cmp(&y.delta).then_with(|| x.path.cmp(&y.path)));
        grew.truncate(o.limit);
        shrank.truncate(o.limit);

        let (fa, fb) = (self.sizes("files", a.id)?, self.sizes("files", b.id)?);
        let mut files_up = Vec::new();
        let mut files_down = Vec::new();
        let keys: HashSet<&String> = fa.keys().chain(fb.keys()).collect();
        for key in keys {
            let (before, after) = (fa.get(key), fb.get(key));
            let include = match (before, after) {
                // appeared: it would have been recorded before had it been this big
                (None, Some(x)) => x.1 >= a.file_floor,
                (Some(x), None) => x.1 >= b.file_floor,
                (Some(x), Some(y)) => (to_i64(y.1) - to_i64(x.1)).abs() >= min,
                (None, None) => false,
            };
            if !include {
                continue;
            }
            let (bs, as_) = (before.map_or(0, |x| x.1), after.map_or(0, |x| x.1));
            let change = FileChange {
                path: after.or(before).map(|x| x.0.clone()).unwrap_or_default(),
                before: bs,
                after: as_,
                delta: to_i64(as_) - to_i64(bs),
                mtime: after.or(before).map_or(0, |x| x.2),
            };
            if change.delta > 0 {
                files_up.push(change);
            } else if change.delta < 0 {
                files_down.push(change);
            }
        }
        files_up.sort_by(|x, y| y.delta.cmp(&x.delta).then_with(|| x.path.cmp(&y.path)));
        files_down.sort_by(|x, y| x.delta.cmp(&y.delta).then_with(|| x.path.cmp(&y.path)));
        files_up.truncate(o.limit);
        files_down.truncate(o.limit);

        let free_delta = (a.volume == b.volume
            && !a.volume.is_empty()
            && a.volume_total > 0
            && b.volume_total > 0)
            .then(|| to_i64(b.volume_free) - to_i64(a.volume_free));
        Ok(Diff {
            on_disk_delta: to_i64(b.on_disk) - to_i64(a.on_disk),
            files_delta: to_i64(b.files) - to_i64(a.files),
            free_delta,
            older: a,
            newer: b,
            grew,
            shrank,
            files_up,
            files_down,
        })
    }

    /// Fit the free space of `volume` over the last `window_days` (plus the live
    /// reading `now_free`/`now_total` at `now`). `None` with fewer than three
    /// readings or less than one day between the first and the last.
    ///
    /// # Errors
    /// SQLite read failure.
    pub fn forecast(
        &self,
        volume: &str,
        now: i64,
        now_free: u64,
        now_total: u64,
        window_days: i64,
    ) -> Result<Option<Forecast>> {
        let mut stmt = self.conn.prepare(
            "SELECT started_at, volume_free FROM scans
             WHERE volume = ?1 AND volume_total > 0 AND started_at >= ?2
             ORDER BY started_at",
        )?;
        let mut points: Vec<(f64, f64)> = stmt
            .query_map(
                params![volume.to_lowercase(), now - window_days * 86_400],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(|(t, f)| (t as f64 / 86_400.0, f as f64))
            .collect();
        if now_total > 0 {
            points.push((now as f64 / 86_400.0, now_free as f64));
        }
        Ok(fit(&points).map(|(slope, span)| Forecast {
            volume: volume.to_lowercase(),
            free: now_free,
            total: now_total,
            bytes_per_day: slope,
            days_to_full: (slope < 0.0).then(|| now_free as f64 / -slope),
            samples: points.len(),
            span_days: span,
        }))
    }
}

/// Least-squares slope (per day) and time span of `(day, bytes)` points; `None`
/// when there are fewer than 3 points or they span less than a day.
fn fit(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.len() < 3 {
        return None;
    }
    let (min_t, max_t) = points
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &(t, _)| {
            (lo.min(t), hi.max(t))
        });
    let span = max_t - min_t;
    if span < 1.0 {
        return None;
    }
    let n = points.len() as f64;
    let mt = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mb = points.iter().map(|p| p.1).sum::<f64>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for &(t, b) in points {
        num += (t - mt) * (b - mb);
        den += (t - mt) * (t - mt);
    }
    (den > 0.0).then(|| (num / den, span))
}

#[cfg(test)]
mod tests;
