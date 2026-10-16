//! Keeper selection, the planner's "one copy always stays" invariant (proptest) and
//! the check right before each copy is acted on, through the real executor.

use std::fs;
use std::path::{Path, PathBuf};

use nfd_core::exec::{self, ExecOptions, ItemStatus, QuarantineRoots, RestoreStatus};
use nfd_core::safety::{LiveProbe, Origin, SafetyGate};
use nfd_core::scanner::{self, ScanOptions};
use nfd_dupes::hash::LiveSource;
use nfd_dupes::keep::{KeepStrategy, Selection, auto_select, keeper};
use nfd_dupes::plan::{Action, PlanError, Verifier, plan};
use nfd_dupes::{CopyFlags, DupeGroup, DupeOptions, DupeReport, FileCopy, find};
use proptest::prelude::*;

fn copy(path: &str, mtime: i64, flags: CopyFlags) -> FileCopy {
    FileCopy {
        path: PathBuf::from(path),
        mtime,
        volume: 1,
        file_id: None,
        links: Vec::new(),
        flags,
    }
}

fn group(id: u32, copies: Vec<FileCopy>) -> DupeGroup {
    DupeGroup {
        id,
        hash: "00".repeat(32),
        size: 100,
        wasted: 100 * (copies.len() as u64 - 1),
        copies,
    }
}

const PLAIN: CopyFlags = CopyFlags {
    cloud: false,
    git: false,
    app_dir: false,
    readonly: false,
};

#[test]
fn strategies_pick_the_expected_copy() {
    let g = group(
        1,
        vec![
            copy("C:/Users/u/Downloads/photo.jpg", 100, PLAIN),
            copy("D:/Photos/2019/photo.jpg", 200, PLAIN),
            copy("D:/Photos/photo (copy).jpg", 300, PLAIN),
            copy("C:/x/p.jpg", 400, PLAIN),
        ],
    );
    let name = |s: &KeepStrategy| {
        g.copies[keeper(&g, s).expect("keeper")]
            .path
            .to_string_lossy()
            .into_owned()
    };
    // oldest outside Downloads
    assert_eq!(name(&KeepStrategy::Auto), "D:/Photos/2019/photo.jpg");
    assert_eq!(
        name(&KeepStrategy::Oldest),
        "C:/Users/u/Downloads/photo.jpg"
    );
    assert_eq!(name(&KeepStrategy::Newest), "C:/x/p.jpg");
    assert_eq!(name(&KeepStrategy::Shortest), "C:/x/p.jpg");
    assert_eq!(
        name(&"prefer:D:/Photos".parse().expect("strategy")),
        "D:/Photos/2019/photo.jpg"
    );
    assert_eq!(
        name(&KeepStrategy::Prefer(vec![PathBuf::from("c:/X")])),
        "C:/x/p.jpg"
    );
    assert!("prefer:".parse::<KeepStrategy>().is_err());
    assert!("biggest".parse::<KeepStrategy>().is_err());
}

#[test]
fn risky_copies_are_kept_first_and_never_preselected() {
    let cloud = CopyFlags {
        cloud: true,
        ..PLAIN
    };
    let git = CopyFlags { git: true, ..PLAIN };
    let groups = vec![
        group(
            1,
            vec![
                copy("D:/a/x.bin", 1, PLAIN),
                copy("D:/OneDrive/x.bin", 5, cloud),
                copy("D:/b/x.bin", 2, PLAIN),
            ],
        ),
        group(
            2,
            vec![
                copy("D:/repo/y.bin", 1, git),
                copy("D:/OneDrive/y.bin", 2, cloud),
            ],
        ),
    ];
    let sel = auto_select(&groups, &KeepStrategy::Newest);
    // group 2: nothing may be pre-selected
    assert_eq!(sel.len(), 1);
    assert_eq!(sel[0].keep, PathBuf::from("D:/OneDrive/x.bin"));
    assert_eq!(
        sel[0].remove,
        vec![PathBuf::from("D:/a/x.bin"), PathBuf::from("D:/b/x.bin")]
    );
}

#[test]
fn the_planner_refuses_to_empty_a_group() {
    let groups = vec![group(
        7,
        vec![
            copy("D:/a", 1, PLAIN),
            copy("D:/b", 1, PLAIN),
            copy("D:/c", 1, PLAIN),
        ],
    )];
    let sel = |keep: &str, remove: &[&str]| Selection {
        group: 7,
        keep: PathBuf::from(keep),
        remove: remove.iter().map(PathBuf::from).collect(),
    };
    assert_eq!(
        plan(&groups, &[sel("D:/a", &["D:/a", "D:/b"])], Action::Recycle),
        Err(PlanError::NothingKept(7))
    );
    assert!(matches!(
        plan(&groups, &[sel("D:/z", &["D:/b"])], Action::Recycle),
        Err(PlanError::NotInGroup { .. })
    ));
    assert!(matches!(
        plan(&groups, &[sel("D:/a", &["D:/elsewhere"])], Action::Recycle),
        Err(PlanError::NotInGroup { .. })
    ));
    assert_eq!(
        plan(
            &groups,
            &[sel("D:/a", &["D:/b"]), sel("D:/a", &["D:/c"])],
            Action::Recycle
        ),
        Err(PlanError::SelectedTwice(7))
    );
    let mut other = sel("D:/a", &["D:/b"]);
    other.group = 8;
    assert_eq!(
        plan(&groups, &[other], Action::Recycle),
        Err(PlanError::UnknownGroup(8))
    );
    let ok = plan(
        &groups,
        &[sel("D:/a", &["D:/b", "D:/c", "D:/b"])],
        Action::Quarantine,
    )
    .expect("plan");
    assert_eq!(ok.items.len(), 2);
    assert_eq!(ok.selected_bytes(), 200);
    assert!(
        ok.items
            .iter()
            .all(|i| i.keeper.as_deref() == Some(Path::new("D:/a"))
                && i.origin == Origin::Duplicate { group: 7 }
                && i.mode == nfd_core::plan::DeleteMode::Quarantine)
    );
}

#[test]
fn every_name_of_a_removed_copy_goes() {
    let mut linked = copy("D:/b", 1, PLAIN);
    linked.links.push(PathBuf::from("D:/b-link"));
    let groups = vec![group(1, vec![copy("D:/a", 1, PLAIN), linked])];
    let p = plan(
        &groups,
        &[Selection {
            group: 1,
            keep: PathBuf::from("D:/a"),
            remove: vec![PathBuf::from("D:/b-link")],
        }],
        Action::Recycle,
    )
    .expect("plan");
    let paths: Vec<_> = p.items.iter().map(|i| i.path.clone()).collect();
    assert_eq!(paths, [PathBuf::from("D:/b"), PathBuf::from("D:/b-link")]);
    assert_eq!(p.selected_bytes(), 100);
}

fn arb_groups() -> impl Strategy<Value = Vec<DupeGroup>> {
    prop::collection::vec(
        prop::collection::vec((0i64..5, any::<[bool; 4]>(), 0usize..3), 2..6),
        1..6,
    )
    .prop_map(|groups| {
        groups
            .into_iter()
            .enumerate()
            .map(|(gi, copies)| {
                let copies = copies
                    .into_iter()
                    .enumerate()
                    .map(|(ci, (mtime, f, dir))| {
                        let dirs = ["D:/Downloads", "D:/keep", "D:/x/y"];
                        let mut c = copy(
                            &format!("{}/g{gi}c{ci}.bin", dirs[dir]),
                            mtime,
                            CopyFlags {
                                cloud: f[0] && f[1],
                                git: f[2] && f[3],
                                app_dir: f[0] && f[2],
                                readonly: f[1] && f[3],
                            },
                        );
                        if f[0] && !f[1] {
                            c.links.push(PathBuf::from(format!("D:/l/g{gi}c{ci}")));
                        }
                        c
                    })
                    .collect();
                group(gi as u32 + 1, copies)
            })
            .collect()
    })
}

proptest! {
    /// PHASE5 DoD: whatever is selected, a plan never removes every copy of a group
    /// and never contains the keeper.
    #[test]
    fn a_plan_always_keeps_one_copy(
        groups in arb_groups(),
        picks in prop::collection::vec((0usize..8, prop::collection::vec(0usize..8, 0..8)), 0..8),
    ) {
        let selections: Vec<Selection> = picks
            .iter()
            .enumerate()
            .filter_map(|(n, (keep, remove))| {
                let g = groups.get(n % groups.len())?;
                let name = |i: usize| {
                    let c = &g.copies[i % g.copies.len()];
                    // sometimes name a copy through its hard link
                    if i % 3 == 0 { c.links.first().unwrap_or(&c.path).clone() } else { c.path.clone() }
                };
                Some(Selection {
                    group: g.id,
                    keep: name(*keep),
                    remove: remove.iter().map(|&i| name(i)).collect(),
                })
            })
            .collect();
        if let Ok(p) = plan(&groups, &selections, Action::Recycle) {
            for g in &groups {
                let gone = |c: &FileCopy| p.items.iter().any(|i| i.path == c.path);
                prop_assert!(g.copies.iter().any(|c| !gone(c)), "group {} emptied", g.id);
                for item in p.items.iter().filter(|i| i.origin == Origin::Duplicate { group: g.id }) {
                    let k = item.keeper.as_ref().expect("keeper");
                    prop_assert!(p.items.iter().all(|i| &i.path != k));
                    prop_assert!(g.copies.iter().any(|c| &c.path == k));
                }
            }
        }
        // the default selection always plans, and never removes a risky copy
        let auto = auto_select(&groups, &KeepStrategy::Auto);
        let p = plan(&groups, &auto, Action::Recycle).expect("auto selection plans");
        for g in &groups {
            prop_assert!(g.copies.iter().any(|c| p.items.iter().all(|i| i.path != c.path)));
            for c in g.copies.iter().filter(|c| nfd_dupes::keep::sticky(c)) {
                prop_assert!(p.items.iter().all(|i| i.path != c.path));
            }
        }
    }
}

// ---------------------------------------------------------------- live

fn search(root: &Path) -> DupeReport {
    let tree = scanner::scan(root, &ScanOptions::default(), None).expect("scan");
    find(
        &[&tree],
        &SafetyGate::new(&|_: &str| None),
        &DupeOptions {
            min_size: 1,
            ..DupeOptions::default()
        },
        None,
        &LiveSource,
        &|_| {},
    )
    .expect("find")
}

fn setup() -> (tempfile::TempDir, PathBuf, Vec<u8>) {
    let dir = tempfile::tempdir().expect("tmp");
    let data = dir.path().join("data");
    fs::create_dir_all(data.join("Downloads")).expect("mkdir");
    let bytes: Vec<u8> = (0..120_000u32).map(|i| (i * 7 % 251) as u8).collect();
    for p in ["orig.bin", "Downloads/orig.bin", "Downloads/orig (1).bin"] {
        fs::write(data.join(p), &bytes).expect("write");
    }
    (dir, data, bytes)
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

fn run(dir: &Path, report: &DupeReport) -> exec::Operation {
    let sel = auto_select(&report.groups, &KeepStrategy::Auto);
    let p = plan(&report.groups, &sel, Action::Quarantine).expect("plan");
    let gate = SafetyGate::new(&|_: &str| None);
    let mut verifier = Verifier::new(&LiveSource, report.groups.clone());
    exec::execute_verified(
        &p,
        &gate,
        &LiveProbe,
        &opts(dir),
        &mut |item| verifier.check(item),
        &mut |_, _, _| true,
    )
}

#[test]
fn copies_go_to_the_quarantine_and_come_back() {
    let (dir, data, bytes) = setup();
    let report = search(&data);
    assert_eq!(report.groups.len(), 1);
    let op = run(dir.path(), &report);
    assert_eq!(op.count(ItemStatus::Done), 2, "{op:#?}");
    assert_eq!(fs::read(data.join("orig.bin")).expect("keeper"), bytes);
    assert!(!data.join("Downloads/orig.bin").exists());
    for r in &op.items {
        let (status, _) = exec::restore(&r.path, r.stored.as_deref(), r.mode, op.started_at);
        assert_eq!(status, RestoreStatus::Restored);
        assert_eq!(fs::read(&r.path).expect("restored"), bytes);
    }
}

#[test]
fn a_changed_keeper_skips_the_whole_group() {
    let (dir, data, bytes) = setup();
    let report = search(&data);
    // same size, other bytes, other date
    let mut other = bytes.clone();
    other[60_000] ^= 0xff;
    fs::write(data.join("orig.bin"), &other).expect("write");
    let op = run(dir.path(), &report);
    assert!(
        op.items
            .iter()
            .all(|r| r.status == ItemStatus::Skipped
                && r.reason.as_deref() == Some("keeper-changed")),
        "{op:#?}"
    );
    assert!(data.join("Downloads/orig.bin").exists());
    assert!(data.join("Downloads/orig (1).bin").exists());
}

#[test]
fn a_changed_copy_is_skipped_and_the_rest_goes() {
    let (dir, data, bytes) = setup();
    let report = search(&data);
    let target = data.join("Downloads/orig (1).bin");
    let mut other = bytes.clone();
    other[0] ^= 1;
    fs::write(&target, &other).expect("write");
    let op = run(dir.path(), &report);
    let status = |end: &str| {
        op.items
            .iter()
            .find(|r| r.path.ends_with(end))
            .map(|r| (r.status, r.reason.clone()))
            .expect("item")
    };
    assert_eq!(status("orig (1).bin").0, ItemStatus::Skipped);
    assert_eq!(status("Downloads/orig.bin").0, ItemStatus::Done);
    // gate reports `changed` (date), the verifier `copy-changed`: either way it stays
    assert!(target.exists());
    assert!(data.join("orig.bin").exists());
}

#[test]
fn a_deleted_keeper_never_lets_the_last_copy_go() {
    let (dir, data, _) = setup();
    let report = search(&data);
    fs::remove_file(data.join("orig.bin")).expect("rm");
    let op = run(dir.path(), &report);
    assert_eq!(op.count(ItemStatus::Done), 0, "{op:#?}");
    assert!(data.join("Downloads/orig.bin").exists());
    assert!(data.join("Downloads/orig (1).bin").exists());
}
