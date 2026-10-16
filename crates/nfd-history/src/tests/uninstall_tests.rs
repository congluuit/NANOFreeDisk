//! Uninstall cleanup: count, restore or purge what is still quarantined.

use std::fs;
use std::path::PathBuf;

use nfd_core::exec::{self, ExecOptions, QuarantineRoots};
use nfd_core::plan::{CleanPlan, DeleteMode};
use nfd_core::safety::{LiveProbe, SafetyGate};

use super::journal_tests::{fixture, item};
use crate::History;
use crate::uninstall;

fn run_modes(
    gate: &SafetyGate,
    roots: &QuarantineRoots,
    id: &str,
    items: &[(PathBuf, DeleteMode)],
) -> exec::Operation {
    let plan = CleanPlan {
        items: items
            .iter()
            .map(|(p, mode)| {
                let mut i = item(p);
                i.mode = *mode;
                i
            })
            .collect(),
    };
    let opts = ExecOptions {
        op_id: id.into(),
        kind: "clean".into(),
        owner: 0,
        roots: roots.clone(),
        now: exec::unix_now(),
    };
    exec::execute(&plan, gate, &LiveProbe, &opts, &mut |_, _, _| true)
}

#[test]
fn restore_all_puts_quarantined_files_back() {
    let f = fixture();
    let a = f.dir.path().join("a.tmp");
    let b = f.dir.path().join("b.tmp");
    fs::write(&a, b"aaa").expect("write");
    fs::write(&b, b"bb").expect("write");
    let op = run_modes(
        &f.gate,
        &f.roots,
        "20261016-000001-0001",
        &[
            (a.clone(), DeleteMode::Quarantine),
            (b.clone(), DeleteMode::Quarantine),
        ],
    );
    let mut h = History::open_in_memory().expect("db");
    h.record_operation(&op).expect("record");

    let p = uninstall::pending(&h).expect("pending");
    assert_eq!(p.items, 2);
    assert_eq!(p.operations, vec![op.id.clone()]);
    assert!(p.bytes >= 2);

    let r = uninstall::restore_all(&mut h).expect("restore");
    assert_eq!(r.restored, 2);
    assert!(r.failed.is_empty());
    assert_eq!(fs::read(&a).expect("a back"), b"aaa");
    assert_eq!(fs::read(&b).expect("b back"), b"bb");
    assert!(uninstall::pending(&h).expect("pending").is_empty());
}

#[test]
fn purge_all_empties_the_quarantine_and_orphans() {
    let f = fixture();
    let a = f.dir.path().join("a.tmp");
    fs::write(&a, b"aaa").expect("write");
    let op = run_modes(
        &f.gate,
        &f.roots,
        "20261016-000001-0002",
        &[(a.clone(), DeleteMode::Quarantine)],
    );
    let root = f.roots.system.clone().expect("root");
    let orphan = root.join("20261015-000000-0009");
    fs::create_dir_all(orphan.join("0")).expect("orphan");
    let mut h = History::open_in_memory().expect("db");
    h.record_operation(&op).expect("record");

    uninstall::purge_all(&mut h, &f.roots).expect("purge");
    assert!(!a.exists());
    assert!(!root.join(&op.id).exists());
    assert!(!orphan.exists());
    assert!(uninstall::pending(&h).expect("pending").is_empty());
}

#[test]
fn nothing_pending_without_quarantine() {
    let h = History::open_in_memory().expect("db");
    assert!(uninstall::pending(&h).expect("pending").is_empty());
}

#[test]
fn remove_data_dir_keeps_a_quarantine_that_still_holds_files() {
    let dir = tempfile::tempdir().expect("tmp");
    let data = dir.path().join("vn.io.nanodev.nanofreedisk");
    let q = data.join("quarantine");
    fs::create_dir_all(q.join("op").join("0")).expect("mkdir");
    fs::write(q.join("op").join("0").join("f"), b"x").expect("write");
    fs::write(data.join("history.db"), b"db").expect("write");
    fs::create_dir_all(data.join("EBWebView").join("Default")).expect("mkdir");

    uninstall::remove_data_dir(&data, Some(&q)).expect("remove");
    assert!(q.join("op").join("0").join("f").exists());
    assert!(!data.join("history.db").exists());
    assert!(!data.join("EBWebView").exists());

    uninstall::remove_data_dir(&data, None).expect("remove all");
    assert!(!data.exists());
    // missing is fine
    uninstall::remove_data_dir(&data, None).expect("again");
}

#[test]
fn portable_flag_moves_data_next_to_the_exe() {
    let dir = tempfile::tempdir().expect("tmp");
    let exe = dir.path().join("nanofreedisk.exe");
    assert_eq!(crate::portable_dir_for(&exe), None);
    fs::write(dir.path().join(crate::PORTABLE_FLAG), b"portable").expect("flag");
    assert_eq!(crate::portable_dir_for(&exe), Some(dir.path().join("data")));
}
