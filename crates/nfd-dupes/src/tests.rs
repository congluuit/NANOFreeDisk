use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use nfd_core::scanner::{self, ScanOptions};

use super::*;
use crate::cache::MemoryCache;
use crate::hash::{LiveInfo, LiveSource, ReadSeek};

/// The real disk, plus paths that pretend to be cloud placeholders, and a log of
/// every file opened for reading.
#[derive(Default)]
struct Spy {
    cloud: Vec<PathBuf>,
    opened: Mutex<Vec<PathBuf>>,
}

impl Source for Spy {
    fn info(&self, path: &Path) -> io::Result<LiveInfo> {
        let mut i = LiveSource.info(path)?;
        i.cloud |= self.cloud.iter().any(|c| c == path);
        Ok(i)
    }

    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadSeek>> {
        self.opened.lock().expect("lock").push(path.to_path_buf());
        LiveSource.open(path)
    }

    fn seek_penalty(&self, _: &Path) -> Option<bool> {
        Some(false)
    }
}

fn tree(root: &Path) -> ScanTree {
    scanner::scan(root, &ScanOptions::default(), None).expect("scan")
}

fn no_gate() -> SafetyGate {
    SafetyGate::new(&|_: &str| None)
}

fn opts(min: u64) -> DupeOptions {
    DupeOptions {
        min_size: min,
        ..DupeOptions::default()
    }
}

fn run_with(
    root: &Path,
    gate: &SafetyGate,
    o: &DupeOptions,
    cache: Option<&mut dyn HashCache>,
    spy: &Spy,
) -> DupeReport {
    let t = tree(root);
    find(&[&t], gate, o, cache, spy, &|_| {}).expect("find")
}

fn names(r: &DupeReport, root: &Path) -> Vec<Vec<String>> {
    r.groups
        .iter()
        .map(|g| {
            g.copies
                .iter()
                .map(|c| {
                    c.path
                        .strip_prefix(root)
                        .expect("inside")
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect()
        })
        .collect()
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

#[test]
fn identical_files_group_and_near_misses_do_not() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let x = pattern(100_000, 1);
    fs::create_dir_all(r.join("a/b")).expect("mkdir");
    fs::write(r.join("x1.bin"), &x).expect("w");
    fs::write(r.join("a/b/x2.bin"), &x).expect("w");
    let mut last = x.clone();
    last[99_999] ^= 1;
    fs::write(r.join("last.bin"), &last).expect("w");
    // outside the three 16 KiB samples: only the full hash tells them apart
    let mut middle = x.clone();
    middle[30_000] ^= 1;
    fs::write(r.join("middle.bin"), &middle).expect("w");
    fs::write(r.join("s1.txt"), b"same small").expect("w");
    fs::write(r.join("s2.txt"), b"same small").expect("w");
    fs::write(r.join("e1"), b"").expect("w");
    fs::write(r.join("e2"), b"").expect("w");

    let spy = Spy::default();
    let rep = run_with(r, &no_gate(), &opts(1), None, &spy);
    assert_eq!(
        names(&rep, r),
        [vec!["a/b/x2.bin", "x1.bin"], vec!["s1.txt", "s2.txt"]]
    );
    assert_eq!(rep.groups[0].size, 100_000);
    assert_eq!(rep.groups[0].wasted, 100_000);
    assert_eq!(rep.wasted_bytes, 100_000 + 10);
    assert_eq!(rep.groups[0].hash, hash::hex(blake3::hash(&x).as_bytes()));
    assert_eq!(rep.skipped.get(&SkipReason::Empty), Some(&2));
    assert_eq!(rep.scanned_files, 8);
    assert_eq!(rep.candidates, 6);
    assert_eq!(rep.groups[0].id, 1);

    // paranoid gives the same answer
    let mut o = opts(1);
    o.paranoid = true;
    let p = run_with(r, &no_gate(), &o, None, &Spy::default());
    assert_eq!(names(&p, r), names(&rep, r));

    // the size threshold
    let big = run_with(r, &no_gate(), &opts(50_000), None, &Spy::default());
    assert_eq!(names(&big, r), [vec!["a/b/x2.bin", "x1.bin"]]);
    assert_eq!(big.skipped.get(&SkipReason::Small), Some(&2));
}

#[test]
fn hard_links_are_one_copy() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let x = pattern(70_000, 9);
    fs::write(r.join("a.bin"), &x).expect("w");
    fs::hard_link(r.join("a.bin"), r.join("link.bin")).expect("link");
    let rep = run_with(r, &no_gate(), &opts(1), None, &Spy::default());
    assert!(rep.groups.is_empty(), "{:#?}", rep.groups);

    fs::write(r.join("c.bin"), &x).expect("w");
    let rep = run_with(r, &no_gate(), &opts(1), None, &Spy::default());
    assert_eq!(rep.groups.len(), 1);
    let g = &rep.groups[0];
    assert_eq!(g.copies.len(), 2, "{g:#?}");
    assert_eq!(g.wasted, 70_000);
    let linked = g
        .copies
        .iter()
        .find(|c| !c.links.is_empty())
        .expect("links");
    assert_eq!(linked.links.len(), 1);
}

#[test]
fn cloud_placeholders_are_never_opened() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let x = pattern(80_000, 3);
    for n in ["a.bin", "b.bin", "online-only.bin"] {
        fs::write(r.join(n), &x).expect("w");
    }
    let spy = Spy {
        cloud: vec![r.join("online-only.bin")],
        ..Spy::default()
    };
    let rep = run_with(r, &no_gate(), &opts(1), None, &spy);
    assert_eq!(names(&rep, r), [vec!["a.bin", "b.bin"]]);
    assert_eq!(rep.skipped.get(&SkipReason::Cloud), Some(&1));
    let opened = spy.opened.lock().expect("lock");
    assert!(!opened.is_empty());
    assert!(
        !opened.iter().any(|p| p.ends_with("online-only.bin")),
        "{opened:?}"
    );
}

/// A real file with the offline attribute (what OneDrive "online-only" files carry):
/// left out from the scan's own attributes, before anything is opened.
#[cfg(windows)]
#[test]
fn offline_attribute_from_the_scan_is_honoured() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let x = pattern(60_000, 4);
    for n in ["a.bin", "b.bin", "c.bin"] {
        fs::write(r.join(n), &x).expect("w");
    }
    let status = std::process::Command::new("attrib")
        .arg("+O")
        .arg(r.join("c.bin"))
        .status()
        .expect("attrib");
    assert!(status.success());
    let spy = Spy::default();
    let rep = run_with(r, &no_gate(), &opts(1), None, &spy);
    assert_eq!(names(&rep, r), [vec!["a.bin", "b.bin"]]);
    assert_eq!(rep.skipped.get(&SkipReason::Cloud), Some(&1));
    assert!(
        !spy.opened
            .lock()
            .expect("lock")
            .iter()
            .any(|p| p.ends_with("c.bin"))
    );
}

#[test]
fn second_run_reads_from_the_cache() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let x = pattern(200_000, 5);
    for n in ["a.bin", "b.bin", "c.bin"] {
        fs::write(r.join(n), &x).expect("w");
    }
    let mut old = filetime(r.join("a.bin"));
    old -= 10;
    for n in ["a.bin", "b.bin", "c.bin"] {
        set_mtime(&r.join(n), old);
    }
    let mut cache = MemoryCache::default();
    let first = run_with(r, &no_gate(), &opts(1), Some(&mut cache), &Spy::default());
    assert_eq!(first.groups.len(), 1);
    assert!(first.hashed_bytes >= 3 * 200_000);
    assert_eq!(cache.len(), 3);

    let spy = Spy::default();
    let second = run_with(r, &no_gate(), &opts(1), Some(&mut cache), &spy);
    assert_eq!(second.groups, first.groups);
    assert_eq!(second.cache_hits, 3);
    assert_eq!(second.hashed_bytes, 0);
    assert!(spy.opened.lock().expect("lock").is_empty());

    // a changed file is hashed again
    fs::write(r.join("c.bin"), pattern(200_000, 6)).expect("w");
    let third = run_with(r, &no_gate(), &opts(1), Some(&mut cache), &Spy::default());
    assert_eq!(names(&third, r), [vec!["a.bin", "b.bin"]]);
}

#[test]
fn the_history_database_is_a_cache_too() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path().join("data");
    fs::create_dir_all(&r).expect("mkdir");
    let x = pattern(64_000, 8);
    for n in ["a.bin", "b.bin"] {
        fs::write(r.join(n), &x).expect("w");
        set_mtime(&r.join(n), 1_700_000_000);
    }
    let mut db = nfd_history::History::open(&dir.path().join("h.db")).expect("db");
    let first = run_with(&r, &no_gate(), &opts(1), Some(&mut db), &Spy::default());
    assert_eq!(first.groups.len(), 1);
    let second = run_with(&r, &no_gate(), &opts(1), Some(&mut db), &Spy::default());
    assert_eq!(second.cache_hits, 2);
    assert_eq!(second.groups, first.groups);
}

#[test]
fn files_changed_after_the_scan_never_group() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let x = pattern(90_000, 7);
    for n in ["a.bin", "b.bin", "c.bin"] {
        fs::write(r.join(n), &x).expect("w");
    }
    let t = tree(r);
    // rewritten with other contents of the same size, later date
    fs::write(r.join("c.bin"), pattern(90_000, 1)).expect("w");
    set_mtime(&r.join("c.bin"), filetime(r.join("a.bin")) + 100);
    let rep = find(&[&t], &no_gate(), &opts(1), None, &Spy::default(), &|_| {}).expect("find");
    assert_eq!(names(&rep, r), [vec!["a.bin", "b.bin"]]);
    assert_eq!(rep.skipped.get(&SkipReason::Changed), Some(&1));
}

#[test]
fn protected_and_application_folders_are_left_out() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let x = pattern(50_000, 2);
    let home = r.join("home");
    for p in [
        "home/.claude/projects/app/s1.jsonl",
        "home/.claude/projects/app/s2.jsonl",
        "home/work/a.bin",
        "home/work/node_modules/pkg/a.bin",
        "home/.ssh/id_copy",
    ] {
        let p = r.join(p);
        fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        fs::write(&p, &x).expect("w");
    }
    fs::write(home.join("work/b.bin"), &x).expect("w");
    let h = home.to_string_lossy().replace('\\', "/");
    let gate = SafetyGate::new(&move |v: &str| (v == "HOME").then(|| h.clone()));
    let rep = run_with(r, &gate, &opts(1), None, &Spy::default());
    assert_eq!(names(&rep, r), [vec!["home/work/a.bin", "home/work/b.bin"]]);
    assert_eq!(rep.skipped.get(&SkipReason::Protected), Some(&3));
    assert_eq!(rep.skipped.get(&SkipReason::AppDir), Some(&1));

    let mut o = opts(1);
    o.include_app_dirs = true;
    let with_apps = run_with(r, &gate, &o, None, &Spy::default());
    let g = &with_apps.groups[0];
    assert_eq!(g.copies.len(), 3);
    assert!(
        g.copies
            .iter()
            .any(|c| c.flags.app_dir && c.path.ends_with("a.bin"))
    );
}

#[test]
fn git_and_cloud_folders_are_flagged() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let x = pattern(50_000, 11);
    for p in [
        "repo/.git/HEAD",
        "repo/src/a.bin",
        "OneDrive/a.bin",
        "plain/a.bin",
    ] {
        let p = r.join(p);
        fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        fs::write(&p, &x).expect("w");
    }
    let rep = run_with(r, &no_gate(), &opts(1), None, &Spy::default());
    let g = &rep.groups[0];
    assert_eq!(g.copies.len(), 3, "{g:#?}");
    let flags = |end: &str| {
        g.copies
            .iter()
            .find(|c| c.path.ends_with(end))
            .expect("copy")
            .flags
    };
    assert!(flags("repo/src/a.bin").git);
    assert!(flags("OneDrive/a.bin").cloud);
    assert_eq!(flags("plain/a.bin"), CopyFlags::default());
}

#[test]
fn extension_filter_and_cancel() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let x = pattern(50_000, 12);
    for n in ["a.jpg", "b.JPG", "c.png", "d.png"] {
        fs::write(r.join(n), &x).expect("w");
    }
    let mut o = opts(1);
    o.extensions = vec![".jpg".into()];
    let rep = run_with(r, &no_gate(), &o, None, &Spy::default());
    assert_eq!(names(&rep, r), [vec!["a.jpg", "b.JPG"]]);
    assert_eq!(rep.skipped.get(&SkipReason::Extension), Some(&2));

    let mut o = opts(1);
    o.cancel = Some(Arc::new(AtomicBool::new(true)));
    let t = tree(r);
    assert_eq!(
        find(&[&t], &no_gate(), &o, None, &Spy::default(), &|_| {}).err(),
        Some(Cancelled)
    );
}

#[test]
fn json_shape() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    fs::write(r.join("a"), b"0123456789").expect("w");
    fs::write(r.join("b"), b"0123456789").expect("w");
    let rep = run_with(r, &no_gate(), &opts(1), None, &Spy::default());
    let v = serde_json::to_value(&rep).expect("json");
    let copy = &v["groups"][0]["copies"][0];
    assert!(copy["volume"].is_string());
    assert!(copy["fileId"].is_string() || copy["fileId"].is_null());
    assert_eq!(v["wastedBytes"], 10);
    let back: FileCopy = serde_json::from_value(copy.clone()).expect("round trip");
    assert_eq!(back, rep.groups[0].copies[0]);
}

fn filetime(p: PathBuf) -> i64 {
    nfd_platform::fsinfo::file_info(&p).expect("info").mtime
}

fn set_mtime(p: &Path, unix: i64) {
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix as u64);
    let f = fs::OpenOptions::new().write(true).open(p).expect("open");
    f.set_modified(t).expect("set mtime");
}
