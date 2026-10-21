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

#[test]
fn quarantined_items_can_be_deleted_for_good_one_by_one() {
    let f = fixture();
    let a = f.dir.path().join("a.tmp");
    let b = f.dir.path().join("cache");
    fs::write(&a, b"aaa").expect("write");
    fs::create_dir(&b).expect("mkdir");
    fs::write(b.join("x"), b"x").expect("write");
    let now = exec::unix_now();
    let op = run(&f, "20261021-000001-0001", now, &[a.clone(), b.clone()]);
    let mut h = History::open_in_memory().expect("db");
    h.record_operation(&op).expect("record");

    let sum = h.quarantine_summary().expect("summary");
    assert_eq!((sum.items, sum.operations), (2, 1));
    assert!(sum.bytes > 0);
    assert_eq!(
        sum.next_expiry,
        Some(now + exec::QUARANTINE_TTL_DAYS * 86_400)
    );
    let row = &h.operations(5).expect("list")[0];
    assert_eq!((row.quarantined, row.restorable), (2, 2));
    assert_eq!(row.expires_at, sum.next_expiry);

    let op_dir = f.roots.system.clone().expect("root").join(&op.id);
    assert!(op_dir.join("0").exists() && op_dir.join("1").exists());
    let one = h.purge_items(&op.id, Some(&[0])).expect("purge one");
    assert_eq!(one.items, 1);
    assert!(one.failed.is_empty(), "{:?}", one.failed);
    assert!(!op_dir.join("0").exists());
    assert!(op_dir.join("1").exists(), "the other item stays");
    let (row, items) = h.operation(&op.id).expect("op");
    assert_eq!(
        (row.state.as_str(), row.restorable, row.quarantined),
        ("active", 1, 1)
    );
    assert!(items[0].purged && !items[0].restorable(&row.state));
    assert!(!items[1].purged);
    // a purged item is not put back
    assert!(
        h.restore_operation(&op.id, Some(&[0]))
            .expect("noop")
            .is_empty()
    );
    assert!(!a.exists());

    let rest = h.purge_items(&op.id, None).expect("purge rest");
    assert_eq!(rest.items, 1);
    assert!(!op_dir.exists(), "the manifest goes with the last item");
    let row = &h.operations(5).expect("list")[0];
    assert_eq!(
        (row.state.as_str(), row.restorable, row.quarantined),
        ("purged", 0, 0)
    );
    assert_eq!(row.expires_at, None);
    assert_eq!(h.quarantine_summary().expect("summary").items, 0);
    assert!(!b.exists(), "deleting for good never puts anything back");
}

#[test]
fn emptying_the_quarantine_takes_every_operation_and_orphan() {
    let f = fixture();
    let a = f.dir.path().join("a.log");
    let b = f.dir.path().join("b.log");
    fs::write(&a, b"a").expect("write");
    fs::write(&b, b"b").expect("write");
    let now = exec::unix_now();
    let op1 = run(&f, "20261021-000002-0001", now - 86_400, &[a]);
    let op2 = run(&f, "20261021-000003-0002", now, &[b]);
    let root = f.roots.system.clone().expect("root");
    let orphan = root.join("20261020-000000-0009");
    fs::create_dir_all(orphan.join("0")).expect("mkdir");
    let mut h = History::open_in_memory().expect("db");
    h.record_operation(&op1).expect("record");
    h.record_operation(&op2).expect("record");
    assert_eq!(h.quarantine_summary().expect("summary").operations, 2);

    let out = h.empty_quarantine(&f.roots, now).expect("empty");
    assert_eq!(out.items, 2);
    assert!(out.failed.is_empty(), "{:?}", out.failed);
    assert!(!root.join(&op1.id).exists() && !root.join(&op2.id).exists());
    assert!(!orphan.exists());
    assert_eq!(h.quarantine_summary().expect("summary").items, 0);
    for o in h.operations(5).expect("list") {
        assert_eq!(o.state, "purged");
    }
}

/// Registry leftovers: their `.reg` backup is what the quarantine holds; a backup
/// laid out unexpectedly is never deleted, and Recycle Bin items stay restorable.
#[test]
fn registry_backups_and_recycle_bin_items_when_purging() {
    use nfd_core::exec::{ItemResult, ItemStatus, Operation};
    let dir = tempfile::tempdir().expect("tmp");
    let id = "20261021-000004-0001";
    let backups = dir.path().join("registry-backup").join(id);
    fs::create_dir_all(&backups).expect("mkdir");
    let good = backups.join("0.reg");
    let odd = dir.path().join("elsewhere.reg");
    fs::write(&good, b"REGEDIT").expect("write");
    fs::write(&odd, b"REGEDIT").expect("write");
    let item = |n: usize, mode: DeleteMode, stored: Option<PathBuf>| ItemResult {
        n,
        path: dir.path().join(format!("item{n}")),
        rule_id: None,
        mode,
        is_dir: false,
        bytes: 1,
        files: 1,
        freed: 1,
        status: ItemStatus::Done,
        reason: None,
        message: None,
        stored,
        warnings: Vec::new(),
    };
    let op = Operation {
        id: id.into(),
        kind: "uninstall".into(),
        started_at: exec::unix_now(),
        finished_at: exec::unix_now(),
        items: vec![
            item(0, DeleteMode::Registry, Some(good.clone())),
            item(1, DeleteMode::Registry, Some(odd.clone())),
            item(2, DeleteMode::RecycleBin, None),
        ],
        cancelled: false,
    };
    let mut h = History::open_in_memory().expect("db");
    h.record_operation(&op).expect("record");
    let row = &h.operations(5).expect("list")[0];
    assert_eq!((row.restorable, row.quarantined), (3, 2));
    assert_eq!(row.expires_at, None, ".reg backups alone never expire");

    let out = h.purge_items(id, None).expect("purge");
    assert_eq!(out.items, 1);
    assert_eq!(out.failed.len(), 1, "{:?}", out.failed);
    assert!(!good.exists());
    assert!(
        odd.exists(),
        "a backup outside its operation folder is left alone"
    );
    let (row, items) = h.operation(id).expect("op");
    assert!(items[0].purged && !items[1].purged && !items[2].purged);
    assert_eq!(row.state, "active");
    assert_eq!(row.restorable, 2, "the odd backup and the Recycle Bin item");
}
