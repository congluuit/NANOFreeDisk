//! Undo journal: record → list → restore → purge, on a real temporary folder.

use std::fs;
use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::exec::{self, ExecOptions, QuarantineRoots, RestoreStatus};
use nfd_core::plan::{CleanPlan, DeleteMode, PlanItem};
use nfd_core::safety::{LiveProbe, Origin, SafetyGate};

use crate::History;

pub(super) struct Fixture {
    pub(super) dir: tempfile::TempDir,
    pub(super) gate: SafetyGate,
    pub(super) roots: QuarantineRoots,
}

pub(super) fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tmp");
    let base = dir.path().to_string_lossy().replace('\\', "/");
    let resolve = move |v: &str| (v == "LOCALAPPDATA").then(|| base.clone());
    let roots = QuarantineRoots {
        system: Some(dir.path().join("app").join("quarantine")),
        other_volumes: false,
    };
    Fixture {
        gate: SafetyGate::new(&resolve),
        dir,
        roots,
    }
}

pub(super) fn item(path: &Path) -> PlanItem {
    let info = nfd_platform::fsinfo::file_info(path).expect("info");
    PlanItem {
        path: path.to_path_buf(),
        origin: Origin::Rule {
            level: RiskLevel::Safe,
        },
        rule_id: Some("t.rule".into()),
        is_dir: info.is_dir,
        bytes: info.on_disk.max(1),
        files: 1,
        size: if info.is_dir { 0 } else { info.logical },
        mtime: info.mtime,
        requires_closed: Vec::new(),
        mode: DeleteMode::Quarantine,
        selected: true,
        keeper: None,
    }
}

fn run(f: &Fixture, id: &str, started_at: i64, paths: &[PathBuf]) -> exec::Operation {
    let plan = CleanPlan {
        items: paths.iter().map(|p| item(p)).collect(),
    };
    let opts = ExecOptions {
        op_id: id.into(),
        kind: "clean".into(),
        owner: 0,
        roots: f.roots.clone(),
        now: started_at,
    };
    exec::execute(&plan, &f.gate, &LiveProbe, &opts, &mut |_, _, _| true)
}

#[test]
fn record_list_and_restore() {
    let f = fixture();
    let a = f.dir.path().join("a.tmp");
    let b = f.dir.path().join("cache");
    fs::write(&a, b"aaa").expect("write");
    fs::create_dir(&b).expect("mkdir");
    fs::write(b.join("x"), b"x").expect("write");
    let now = exec::unix_now();
    let op = run(&f, "20261016-000001-0001", now, &[a.clone(), b.clone()]);
    assert!(!a.exists() && !b.exists());

    let mut h = History::open_in_memory().expect("db");
    h.record_operation(&op).expect("record");
    let ops = h.operations(10).expect("list");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].done, 2);
    assert_eq!(ops[0].restorable, 2);
    assert_eq!(ops[0].state, "active");

    // restore only the first item
    let first = h
        .restore_operation(&op.id, Some(&[0]))
        .expect("restore one");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].status, RestoreStatus::Restored);
    assert_eq!(fs::read(&a).expect("back"), b"aaa");
    assert_eq!(h.operations(10).expect("list")[0].state, "partly-restored");

    let rest = h.restore_operation(&op.id, None).expect("restore rest");
    assert_eq!(rest.len(), 1);
    assert!(b.join("x").exists());
    let row = &h.operations(10).expect("list")[0];
    assert_eq!(row.state, "restored");
    assert_eq!(row.restorable, 0);
    // nothing left to restore
    assert!(h.restore_operation(&op.id, None).expect("noop").is_empty());
}

#[test]
fn purge_expired_removes_old_quarantine_only() {
    let f = fixture();
    let old = f.dir.path().join("old.tmp");
    let new = f.dir.path().join("new.tmp");
    fs::write(&old, b"1").expect("write");
    fs::write(&new, b"2").expect("write");
    let now = exec::unix_now();
    let op_old = run(&f, "20000101-000000-0001", now - 30 * 86_400, &[old]);
    let op_new = run(&f, "20991231-000000-0002", now, &[new]);
    // a folder nobody remembers (database cleared)
    let root = f.roots.system.clone().expect("root");
    let orphan = root.join("20000102-000000-0003");
    fs::create_dir_all(orphan.join("0")).expect("mkdir");

    let mut h = History::open_in_memory().expect("db");
    h.record_operation(&op_old).expect("record");
    h.record_operation(&op_new).expect("record");
    let purged = h.purge_expired(&f.roots, now, None).expect("purge");
    assert!(purged.contains(&op_old.id), "{purged:?}");
    assert!(purged.iter().any(|p| p == "20000102-000000-0003"));
    assert!(!purged.contains(&op_new.id));
    assert!(!root.join(&op_old.id).exists());
    assert!(!orphan.exists());
    assert!(root.join(&op_new.id).exists());

    let (row, _) = h.operation(&op_old.id).expect("op");
    assert_eq!(row.state, "purged");
    assert_eq!(row.restorable, 0);
    assert!(
        h.restore_operation(&op_old.id, None)
            .expect("noop")
            .is_empty()
    );

    assert_eq!(h.prune_operations(now, 7).expect("prune"), 1);
    assert!(h.operation(&op_old.id).is_err());
}

#[test]
fn version_1_databases_gain_the_journal() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("history.db");
    {
        let conn = rusqlite::Connection::open(&path).expect("open");
        conn.execute_batch(&format!("{} PRAGMA user_version = 1;", crate::SCHEMA))
            .expect("v1 schema");
    }
    let h = History::open(&path).expect("migrate");
    assert!(h.operations(5).expect("journal").is_empty());
    let v: i64 = h
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, crate::SCHEMA_VERSION);
}

#[test]
fn clearing_scan_history_keeps_the_journal() {
    let f = fixture();
    let a = f.dir.path().join("a.tmp");
    fs::write(&a, b"a").expect("write");
    let op = run(&f, "20261016-000002-0004", exec::unix_now(), &[a]);
    let mut h = History::open_in_memory().expect("db");
    h.record_operation(&op).expect("record");
    h.clear().expect("clear");
    assert_eq!(h.operations(5).expect("list").len(), 1);
}

/// Commands (package stores, `git worktree remove`) keep their command line and undo
/// hint; a database written before the column existed gets it added in place.
#[test]
fn command_messages_are_kept() {
    use nfd_core::exec::{ItemResult, ItemStatus, Operation};
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("history.db");
    {
        // an older layout of the items table, without `message`
        let h = History::open(&path).expect("db");
        h.conn
            .execute_batch(
                "DROP TABLE operation_items;
                 CREATE TABLE operation_items (
                    op_id TEXT NOT NULL, n INTEGER NOT NULL, path TEXT NOT NULL, rule_id TEXT,
                    mode TEXT NOT NULL, is_dir INTEGER NOT NULL, bytes INTEGER NOT NULL,
                    files INTEGER NOT NULL, freed INTEGER NOT NULL, status TEXT NOT NULL,
                    reason TEXT, stored TEXT, restored INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (op_id, n)) WITHOUT ROWID;",
            )
            .expect("old layout");
    }
    let mut h = History::open(&path).expect("reopen");
    let item = |n: usize, mode: DeleteMode, message: &str| ItemResult {
        n,
        path: dir.path().join(format!("wt{n}")),
        rule_id: Some("command:git-worktree-remove".into()),
        mode,
        is_dir: true,
        bytes: 10,
        files: 1,
        freed: 10,
        status: ItemStatus::Done,
        reason: None,
        message: Some(message.into()),
        stored: None,
        warnings: Vec::new(),
    };
    let op = Operation {
        id: "20261017-000001-0001".into(),
        kind: "worktree".into(),
        started_at: 1,
        finished_at: 2,
        items: vec![
            item(
                0,
                DeleteMode::Permanent,
                "git worktree remove · undo: git worktree add",
            ),
            item(1, DeleteMode::Quarantine, "not a command"),
        ],
        cancelled: false,
    };
    h.record_operation(&op).expect("record");
    let (row, items) = h.operation(&op.id).expect("read");
    assert_eq!(row.kind, "worktree");
    assert_eq!(
        items[0].message.as_deref(),
        Some("git worktree remove · undo: git worktree add")
    );
    assert_eq!(items[1].message, None);
}
