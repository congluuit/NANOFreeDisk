//! Duplicate folders (PHASE5_DUPLICATES §2.4): identical trees found at the top-most
//! level only, folders with something left out never take part, and a folder is
//! checked against the disk again right before it is removed.

use std::fs;
use std::path::Path;

use nfd_core::exec::{self, ExecOptions, ItemStatus, QuarantineRoots, RestoreStatus};
use nfd_core::safety::{LiveProbe, SafetyGate};
use nfd_core::scanner::{self, ScanOptions};
use nfd_dupes::hash::LiveSource;
use nfd_dupes::keep::{KeepStrategy, Selection, auto_select_folders};
use nfd_dupes::plan::{Action, PlanError, Verifier, plan_folders};
use nfd_dupes::{DupeOptions, DupeReport, find};

fn write(root: &Path, rel: &str, data: &[u8]) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
    fs::write(p, data).expect("write");
}

/// An album with photos, a sub-folder, a tiny text file and an empty file.
fn album(root: &Path, at: &str) {
    write(root, &format!("{at}/a.jpg"), &[1u8; 30_000]);
    write(root, &format!("{at}/b.jpg"), &[2u8; 40_000]);
    write(root, &format!("{at}/raw/c.dng"), &[3u8; 50_000]);
    write(root, &format!("{at}/notes.txt"), b"trip");
    write(root, &format!("{at}/empty.txt"), b"");
}

fn search(root: &Path, gate: &SafetyGate) -> DupeReport {
    let tree = scanner::scan(root, &ScanOptions::default(), None).expect("scan");
    find(
        &[&tree],
        gate,
        &DupeOptions {
            folders: true,
            ..DupeOptions::default()
        },
        None,
        &LiveSource,
        &|_| {},
    )
    .expect("find")
}

fn no_gate() -> SafetyGate {
    SafetyGate::new(&|_: &str| None)
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .expect("inside")
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn identical_trees_are_found_at_the_top() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    album(r, "Photos/Trip");
    album(r, "Backup/Trip copy");
    album(r, "Old/Trip");
    // same files, one extra: not identical
    album(r, "Other/Trip");
    write(r, "Other/Trip/extra.jpg", &[9u8; 1000]);
    // same names, one byte different
    album(r, "Edited/Trip");
    write(r, "Edited/Trip/raw/c.dng", &[4u8; 50_000]);
    let rep = search(r, &no_gate());
    let names: Vec<Vec<String>> = rep
        .folder_groups
        .iter()
        .map(|g| g.folders.iter().map(|f| rel(r, &f.path)).collect())
        .collect();
    // "Photos/Trip/raw" etc. are inside reported folders: not listed again; the
    // album's `raw` also matches in Other/Trip, so that group stays (it names a
    // folder no other group covers)
    assert!(
        names.contains(&vec![
            "Backup/Trip copy".to_owned(),
            "Old/Trip".to_owned(),
            "Photos/Trip".to_owned()
        ]),
        "{names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|g| g.iter().any(|f| f.starts_with("Edited"))),
        "{names:?}"
    );
    let trip = rep
        .folder_groups
        .iter()
        .find(|g| g.folders.len() == 3)
        .expect("trip group");
    assert_eq!(trip.size, 30_000 + 40_000 + 50_000 + 4);
    assert_eq!(trip.files, 4);
    assert_eq!(trip.wasted, 2 * trip.size);
    // ids continue after the file groups
    assert!(trip.id as usize > rep.groups.len());
    // the default file report still uses the 1 MiB threshold
    assert!(rep.groups.is_empty(), "{:?}", rep.groups);
    assert_eq!(
        rep.folder_wasted,
        rep.folder_groups.iter().map(|g| g.wasted).sum::<u64>()
    );
}

#[test]
fn a_folder_with_something_left_out_never_matches() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    album(r, "A/Trip");
    album(r, "B/Trip");
    // a protected file inside B (an AI conversation log, by name)
    write(r, "A/Trip/.aider.chat.history.md", b"chat");
    write(r, "B/Trip/.aider.chat.history.md", b"chat");
    let rep = search(r, &no_gate());
    assert!(
        !rep.folder_groups
            .iter()
            .any(|g| g.folders.iter().any(|f| f.path.ends_with("Trip"))),
        "{:?}",
        rep.folder_groups
    );
}

fn opts(dir: &Path) -> ExecOptions {
    ExecOptions {
        op_id: exec::new_op_id(exec::unix_now()),
        kind: "dupes".into(),
        owner: 0,
        roots: QuarantineRoots {
            system: Some(dir.join("q")),
            other_volumes: false,
        },
        now: exec::unix_now(),
    }
}

#[test]
fn folders_go_whole_and_come_back() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path().join("data");
    album(&r, "Photos/Trip");
    album(&r, "Downloads/Trip");
    // something else next to each, so the albums are the top-most duplicates
    write(&r, "Photos/cover.jpg", &[7u8; 900]);
    write(&r, "Downloads/setup.exe", &[8u8; 700]);
    let rep = search(&r, &no_gate());
    let sel = auto_select_folders(&rep.folder_groups, &KeepStrategy::Auto);
    assert_eq!(sel.len(), 1);
    assert!(
        sel[0].keep.ends_with("Photos/Trip"),
        "{sel:?} {:?}",
        rep.folder_groups
    );
    // keeping nothing is refused; folders are never linked
    let all = Selection {
        remove: rep.folder_groups[0]
            .folders
            .iter()
            .map(|f| f.path.clone())
            .collect(),
        ..sel[0].clone()
    };
    assert!(matches!(
        plan_folders(&rep.folder_groups, &[all], Action::Recycle),
        Err(PlanError::NothingKept(_))
    ));
    assert!(matches!(
        plan_folders(&rep.folder_groups, &sel, Action::Link),
        Err(PlanError::CannotLink { .. })
    ));
    let p = plan_folders(&rep.folder_groups, &sel, Action::Quarantine).expect("plan");
    assert!(p.items.iter().all(|i| i.is_dir));
    let gate = no_gate();
    let mut verifier =
        Verifier::new(&LiveSource, rep.groups.clone()).with_folders(rep.folder_groups.clone());
    let op = exec::execute_verified(
        &p,
        &gate,
        &LiveProbe,
        &opts(dir.path()),
        &mut |i| verifier.check(i),
        &mut |_, _, _| true,
    );
    assert_eq!(op.count(ItemStatus::Done), 1, "{op:#?}");
    assert!(!r.join("Downloads/Trip").exists());
    assert!(r.join("Photos/Trip/raw/c.dng").exists());
    let item = &op.items[0];
    let (status, _) = exec::restore(&item.path, item.stored.as_deref(), item.mode, op.started_at);
    assert_eq!(status, RestoreStatus::Restored);
    assert_eq!(
        fs::read(r.join("Downloads/Trip/raw/c.dng")).expect("back"),
        vec![3u8; 50_000]
    );
}

#[test]
fn a_folder_that_changed_is_left_alone() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path().join("data");
    album(&r, "Photos/Trip");
    album(&r, "Downloads/Trip");
    // something else next to each, so the albums are the top-most duplicates
    write(&r, "Photos/cover.jpg", &[7u8; 900]);
    write(&r, "Downloads/setup.exe", &[8u8; 700]);
    let rep = search(&r, &no_gate());
    let sel = auto_select_folders(&rep.folder_groups, &KeepStrategy::Auto);
    let p = plan_folders(&rep.folder_groups, &sel, Action::Quarantine).expect("plan");
    // a new file appears in the copy after the search
    write(&r, "Downloads/Trip/new.jpg", b"new");
    let gate = no_gate();
    let mut verifier =
        Verifier::new(&LiveSource, Vec::new()).with_folders(rep.folder_groups.clone());
    let op = exec::execute_verified(
        &p,
        &gate,
        &LiveProbe,
        &opts(dir.path()),
        &mut |i| verifier.check(i),
        &mut |_, _, _| true,
    );
    assert_eq!(op.items[0].status, ItemStatus::Skipped);
    assert_eq!(op.items[0].reason.as_deref(), Some("copy-changed"));
    assert!(r.join("Downloads/Trip/new.jpg").exists());

    // the keeper changes: nothing in its group moves
    fs::remove_file(r.join("Downloads/Trip/new.jpg")).expect("rm");
    write(&r, "Photos/Trip/notes.txt", b"TRIP");
    let mut verifier =
        Verifier::new(&LiveSource, Vec::new()).with_folders(rep.folder_groups.clone());
    let op = exec::execute_verified(
        &p,
        &gate,
        &LiveProbe,
        &opts(dir.path()),
        &mut |i| verifier.check(i),
        &mut |_, _, _| true,
    );
    assert_eq!(op.items[0].reason.as_deref(), Some("keeper-changed"));
    assert!(r.join("Downloads/Trip").exists());
}

#[test]
fn protected_folders_are_kept_and_never_removed() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path().join("home");
    album(&r, "Documents");
    album(&r, "Backup/Docs");
    // the user-data folder itself is protected (only items inside may go)
    let h = r.to_string_lossy().replace('\\', "/");
    let gate = SafetyGate::new(&move |v: &str| (v == "HOME").then(|| h.clone()));
    let rep = search(&r, &gate);
    let g = rep
        .folder_groups
        .iter()
        .find(|g| g.folders.iter().any(|f| f.blocked))
        .expect("group with the Documents folder");
    let sel = auto_select_folders(&rep.folder_groups, &KeepStrategy::Newest);
    let s = sel.iter().find(|s| s.group == g.id).expect("selection");
    assert!(
        s.keep.ends_with("Documents"),
        "the protected folder is the one kept: {s:?}"
    );
    let swapped = Selection {
        group: g.id,
        keep: s.remove[0].clone(),
        remove: vec![s.keep.clone()],
    };
    assert!(matches!(
        plan_folders(&rep.folder_groups, &[swapped], Action::Recycle),
        Err(PlanError::Protected { .. })
    ));
}
