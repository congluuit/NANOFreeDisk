//! Scan reports: the stable JSON document and the CSV export.

use std::io::{self, Write};

use serde::Serialize;

use super::{ExtStat, ScanIssue, ScanMode, ScanStats, ScanTree};
use crate::model::{EntryId, EntryKind};

/// Schema id written into every JSON report.
pub const SCHEMA_ID: &str = "nanofreedisk.scan.v1";

/// The JSON document printed by `nfd scan --json`.
///
/// Schema `nanofreedisk.scan.v1` (machine-readable copy:
/// `docs/schema/scan-v1.schema.json`). Adding fields is backward compatible;
/// renaming or removing one bumps the schema id. All sizes are bytes, all times are
/// Unix seconds (UTC), all paths are absolute and lossily UTF-8.
///
/// ```text
/// {
///   "schema":     "nanofreedisk.scan.v1",
///   "version":    string,                 // nfd version that wrote the report
///   "root":       string,                 // path that was scanned
///   "mode":       "standard" | "fast",    // mode actually used
///   "fallback":   string | null,          // why --fast fell back to standard
///   "scanned_at": integer,                // scan start
///   "elapsed_ms": integer,
///   "top":        integer,                // row limit applied to the three lists
///   "min_size":   integer,                // rows with on_disk below this are omitted
///   "totals": {                           // whole tree, hard-link duplicates excluded
///     "files", "dirs", "symlinks", "junctions", "mount_points", "other",
///     "hardlink_dups", "logical", "on_disk", "errors", "approximate": integer
///   },
///   "children": [                         // direct children of root, on_disk desc
///     { "path": string, "name": string,
///       "kind": "file"|"dir"|"symlink"|"junction"|"mount_point"|"other",
///       "logical": integer, "on_disk": integer, "files": integer }
///   ],
///   "top_files": [                        // largest files, on_disk desc
///     { "path": string, "logical": integer, "on_disk": integer, "mtime": integer,
///       "attrs": ["system"|"hidden"|"reparse"|"compressed"|"sparse"|"offline"|"readonly"],
///       "hardlinks": integer }
///   ],
///   "extensions": [                       // on_disk desc; "" = no extension
///     { "ext": string, "files": integer, "logical": integer, "on_disk": integer }
///   ],
///   "errors": [                           // every unreadable path (not truncated)
///     { "path": string, "kind": "access_denied"|"not_found"|"other", "message": string }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub version: &'static str,
    pub root: String,
    pub mode: ScanMode,
    pub fallback: Option<String>,
    pub scanned_at: i64,
    pub elapsed_ms: u64,
    pub top: usize,
    pub min_size: u64,
    pub totals: ScanStats,
    pub children: Vec<ChildRow>,
    pub top_files: Vec<FileRow>,
    pub extensions: Vec<ExtStat>,
    pub errors: Vec<ScanIssue>,
}

/// A direct child of the scanned root.
#[derive(Debug, Clone, Serialize)]
pub struct ChildRow {
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
    pub logical: u64,
    pub on_disk: u64,
    pub files: u64,
}

/// One of the largest files.
#[derive(Debug, Clone, Serialize)]
pub struct FileRow {
    pub path: String,
    pub logical: u64,
    pub on_disk: u64,
    pub mtime: i64,
    pub attrs: Vec<&'static str>,
    pub hardlinks: u16,
}

impl Report {
    /// Summarise `tree`: `top` rows per list, ignoring entries below `min_size` on disk.
    #[must_use]
    pub fn new(tree: &ScanTree, top: usize, min_size: u64) -> Self {
        let path = |id: EntryId| tree.path(id).to_string_lossy().into_owned();
        let children = tree
            .top_children(top, min_size)
            .into_iter()
            .map(|id| {
                let e = tree.entry(id);
                let (logical, on_disk) = e.counted();
                ChildRow {
                    path: path(id),
                    name: e.name.to_string(),
                    kind: e.kind,
                    logical,
                    on_disk,
                    files: tree.file_counts[id.index()],
                }
            })
            .collect();
        let top_files = tree
            .top_files(top, min_size)
            .into_iter()
            .map(|id| {
                let e = tree.entry(id);
                FileRow {
                    path: path(id),
                    logical: e.logical,
                    on_disk: e.on_disk,
                    mtime: e.mtime,
                    attrs: e.attrs.names(),
                    hardlinks: e.hardlink_count,
                }
            })
            .collect();
        let mut extensions = tree.extensions();
        extensions.retain(|x| x.on_disk >= min_size);
        extensions.truncate(top);

        Self {
            schema: SCHEMA_ID,
            version: env!("CARGO_PKG_VERSION"),
            root: tree.root.to_string_lossy().into_owned(),
            mode: tree.mode,
            fallback: tree.fallback.clone(),
            scanned_at: tree.started_at,
            elapsed_ms: u64::try_from(tree.elapsed.as_millis()).unwrap_or(u64::MAX),
            top,
            min_size,
            totals: tree.stats,
            children,
            top_files,
            extensions,
            errors: tree.issues.clone(),
        }
    }
}

/// CSV header written by [`write_csv`].
pub const CSV_HEADER: &str = "path,kind,logical,on_disk,files,mtime,attrs,hardlink_dup";

/// Export every entry whose counted on-disk size is at least `min_size`, in tree order
/// (RFC 4180 quoting, `\n` line endings). Directory rows carry rolled-up totals;
/// `attrs` is a `|`-separated list.
///
/// # Errors
/// Propagates write errors.
pub fn write_csv(tree: &ScanTree, min_size: u64, out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "{CSV_HEADER}")?;
    for e in &tree.entries {
        let (_, counted_on_disk) = e.counted();
        if counted_on_disk < min_size {
            continue;
        }
        let kind = match e.kind {
            EntryKind::File => "file",
            EntryKind::Dir => "dir",
            EntryKind::Symlink => "symlink",
            EntryKind::Junction => "junction",
            EntryKind::MountPoint => "mount_point",
            EntryKind::Other => "other",
        };
        writeln!(
            out,
            "{},{kind},{},{},{},{},{},{}",
            csv_field(&tree.path(e.id).to_string_lossy()),
            e.logical,
            e.on_disk,
            tree.file_counts[e.id.index()],
            e.mtime,
            e.attrs.names().join("|"),
            e.hardlink_dup,
        )?;
    }
    Ok(())
}

fn csv_field(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\"")).into()
    } else {
        s.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quoting() {
        assert_eq!(csv_field(r"C:\a\b.txt"), r"C:\a\b.txt");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
