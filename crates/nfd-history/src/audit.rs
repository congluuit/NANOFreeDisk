//! Plain-text audit log (PHASE3_CLEAN §2.3): one tab-separated line per item of every
//! operation in `logs\audit-YYYYMMDD.log` next to the history database, kept for
//! [`KEEP_DAYS`] days. Independent of SQLite so it survives a damaged database.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use nfd_core::exec::{ItemStatus, Operation, civil};

/// Days the audit files are kept.
pub const KEEP_DAYS: i64 = 90;

/// `logs` folder next to the history database (`None` when it cannot be resolved).
#[must_use]
pub fn default_dir() -> Option<PathBuf> {
    crate::default_path().and_then(|p| p.parent().map(|d| d.join("logs")))
}

fn stamp(t: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(t);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Append `op` to today's file in `dir` (created if needed) and drop files older
/// than [`KEEP_DAYS`].
///
/// # Errors
/// The folder or file cannot be written.
pub fn append(dir: &Path, op: &Operation) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let (y, m, d, ..) = civil(op.finished_at);
    let path = dir.join(format!("audit-{y:04}{m:02}{d:02}.log"));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let mut text = String::new();
    for i in op
        .items
        .iter()
        .filter(|i| i.status != ItemStatus::NotSelected)
    {
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            stamp(op.finished_at),
            op.id,
            op.kind,
            i.status.as_str(),
            i.mode.as_str(),
            i.freed,
            i.path.display(),
            i.rule_id.as_deref().unwrap_or("-"),
            i.reason.as_deref().unwrap_or("-"),
        ));
    }
    file.write_all(text.as_bytes())?;
    prune(dir, op.finished_at, KEEP_DAYS);
    Ok(path)
}

/// Append a free-form line (restore, purge) to today's file.
///
/// # Errors
/// The folder or file cannot be written.
pub fn note(dir: &Path, now: i64, line: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let (y, m, d, ..) = civil(now);
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("audit-{y:04}{m:02}{d:02}.log")))?;
    writeln!(file, "{}\t{line}", stamp(now))
}

/// Remove `audit-YYYYMMDD.log` files older than `days`.
pub fn prune(dir: &Path, now: i64, days: i64) {
    let (y, m, d, ..) = civil(now - days * 86_400);
    let cutoff = format!("audit-{y:04}{m:02}{d:02}.log");
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("audit-")
            && name.ends_with(".log")
            && name.len() == cutoff.len()
            && name < cutoff
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfd_core::exec::ItemResult;
    use nfd_core::plan::DeleteMode;

    #[test]
    fn writes_lines_and_prunes_old_files() {
        let dir = tempfile::tempdir().expect("tmp");
        let old = dir.path().join("audit-20000101.log");
        fs::write(&old, "old").expect("write");
        let item = |n: usize, status| ItemResult {
            n,
            path: PathBuf::from(format!("/x/{n}")),
            rule_id: Some("r".into()),
            mode: DeleteMode::Quarantine,
            is_dir: false,
            bytes: 5,
            files: 1,
            freed: 5,
            status,
            reason: None,
            message: None,
            stored: None,
            warnings: Vec::new(),
        };
        let op = Operation {
            id: "20261016-000000-abcd".into(),
            kind: "clean".into(),
            started_at: 1_792_150_000,
            finished_at: 1_792_150_000,
            items: vec![item(0, ItemStatus::Done), item(1, ItemStatus::NotSelected)],
            cancelled: false,
        };
        let path = append(dir.path(), &op).expect("append");
        assert!(path.ends_with("audit-20261016.log"));
        let text = fs::read_to_string(&path).expect("read");
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("\tdone\tquarantine\t5\t"));
        assert!(!old.exists());
        note(dir.path(), 1_792_150_000, "restore x").expect("note");
        assert!(
            fs::read_to_string(&path)
                .expect("read")
                .contains("restore x")
        );
    }
}
