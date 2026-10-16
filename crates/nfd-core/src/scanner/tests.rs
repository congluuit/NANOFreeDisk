use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};

use super::*;
use crate::model::EntryKind;

fn write(path: &Path, len: usize) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(path, vec![0xA5u8; len]).expect("write file");
}

fn scan_ok(root: &Path) -> ScanTree {
    scan(root, &ScanOptions::default(), None).expect("scan")
}

fn find<'a>(tree: &'a ScanTree, rel: &str) -> &'a Entry {
    let want = tree.root.join(rel);
    tree.entries
        .iter()
        .find(|e| tree.path(e.id) == want)
        .unwrap_or_else(|| panic!("{rel} not in scan"))
}

/// root/
///   a/one.txt        1000
///   a/b/two.bin      20000
///   a/b/c/three.BIN  300000
///   top.log          5
///   empty/
fn sample_tree(root: &Path) {
    write(&root.join("a/one.txt"), 1000);
    write(&root.join("a/b/two.bin"), 20_000);
    write(&root.join("a/b/c/three.BIN"), 300_000);
    write(&root.join("top.log"), 5);
    fs::create_dir_all(root.join("empty")).expect("mkdir");
}

#[test]
fn nested_tree_rolls_up_sizes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    sample_tree(tmp.path());
    let tree = scan_ok(tmp.path());

    assert_eq!(tree.stats.files, 4);
    assert_eq!(tree.stats.dirs, 5, "root, a, b, c, empty");
    assert_eq!(tree.stats.errors, 0, "{:?}", tree.issues);
    assert_eq!(tree.stats.logical, 321_005);

    let root = &tree.entries[0];
    assert_eq!(root.parent, None);
    assert_eq!(root.kind, EntryKind::Dir);
    assert_eq!(root.logical, 321_005);
    assert_eq!(root.on_disk, tree.stats.on_disk);
    assert!(root.on_disk >= 300_000, "allocation covers the big file");

    assert_eq!(find(&tree, "a").logical, 321_000);
    assert_eq!(find(&tree, "a/b").logical, 320_000);
    assert_eq!(find(&tree, "a/b/c").logical, 300_000);
    assert_eq!(find(&tree, "empty").logical, 0);
    assert_eq!(tree.file_counts[find(&tree, "a").id.index()], 3);
    assert_eq!(tree.file_counts[0], 4);

    // Pre-order arena: every parent precedes its children.
    for e in &tree.entries {
        if let Some(p) = e.parent {
            assert!(p.0 < e.id.0);
            assert_eq!(tree.entry(p).kind, EntryKind::Dir);
        }
    }

    let top: Vec<_> = tree
        .top_files(2, 0)
        .into_iter()
        .map(|id| tree.path(id))
        .collect();
    assert_eq!(
        top,
        [
            tmp.path().join("a/b/c/three.BIN"),
            tmp.path().join("a/b/two.bin")
        ]
    );
    assert_eq!(
        tree.top_files(10, 10_000).len(),
        2,
        "min size filters small files"
    );

    let children: Vec<_> = tree
        .top_children(10, 0)
        .into_iter()
        .map(|id| tree.entry(id).name.to_string())
        .collect();
    assert_eq!(children[0], "a");
    assert_eq!(children.len(), 3);

    let ext = tree.extensions();
    assert_eq!(ext[0].ext, "bin", "extensions are case-folded");
    assert_eq!((ext[0].files, ext[0].logical), (2, 320_000));
    assert!(ext.iter().any(|x| x.ext == "log" && x.files == 1));
}

#[test]
fn single_file_root() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let f = tmp.path().join("only.dat");
    write(&f, 4321);
    let tree = scan_ok(&f);
    assert_eq!(tree.entries.len(), 1);
    assert_eq!(tree.entries[0].kind, EntryKind::File);
    assert_eq!(tree.stats.logical, 4321);
}

#[test]
fn missing_root_is_fatal() {
    let err = scan(
        Path::new("definitely/not/here/nfd"),
        &ScanOptions::default(),
        None,
    );
    assert!(matches!(err, Err(ScanError::Root { .. })));
}

#[test]
fn hard_links_are_counted_once() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let original = tmp.path().join("x/data.bin");
    write(&original, 100_000);
    fs::create_dir_all(tmp.path().join("y")).expect("mkdir");
    let alias = tmp.path().join("y/alias.bin");
    if let Err(e) = fs::hard_link(&original, &alias) {
        eprintln!("skipping: filesystem does not support hard links ({e})");
        return;
    }

    let tree = scan_ok(tmp.path());
    assert_eq!(tree.stats.files, 1);
    assert_eq!(tree.stats.hardlink_dups, 1);
    assert_eq!(tree.stats.logical, 100_000);
    assert_eq!(
        tree.entries[0].logical, 100_000,
        "root total counts the data once"
    );

    let x = find(&tree, "x/data.bin");
    let y = find(&tree, "y/alias.bin");
    assert_eq!(x.hardlink_count, 2);
    assert_eq!(y.hardlink_count, 2);
    assert_eq!(x.file_id, y.file_id);
    assert!(x.file_id.is_some());
    // Sorted walk: x/ comes before y/, so the alias is the duplicate.
    assert!(!x.hardlink_dup);
    assert!(y.hardlink_dup);
    assert_eq!(
        y.logical, 100_000,
        "the duplicate still shows its real size"
    );
    assert_eq!(find(&tree, "x").logical, 100_000);
    assert_eq!(find(&tree, "y").logical, 0);
    assert_eq!(tree.top_files(10, 0).len(), 1);
    assert_eq!(tree.extensions()[0].files, 1);
}

#[test]
fn exact_links_mode_agrees_with_listing_mode() {
    let tmp = tempfile::tempdir().expect("tempdir");
    sample_tree(tmp.path());
    let original = tmp.path().join("a/one.txt");
    if fs::hard_link(&original, tmp.path().join("z.txt")).is_err() {
        eprintln!("skipping: filesystem does not support hard links");
        return;
    }
    let fast = scan_ok(tmp.path());
    let exact = scan(
        tmp.path(),
        &ScanOptions {
            exact_links: true,
            ..ScanOptions::default()
        },
        None,
    )
    .expect("scan");
    assert_eq!(fast.stats, exact.stats);
    assert_eq!(exact.stats.hardlink_dups, 1);
    for tree in [&fast, &exact] {
        assert_eq!(find(tree, "a/one.txt").hardlink_count, 2);
        assert!(find(tree, "z.txt").hardlink_dup);
        assert_eq!(find(tree, "a/b/two.bin").hardlink_count, 1);
    }
}

/// Create a directory junction (Windows) without needing the symlink privilege.
#[cfg(windows)]
fn make_dir_link(link: &Path, target: &Path) -> bool {
    std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(unix)]
fn make_dir_link(link: &Path, target: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[test]
fn directory_links_are_not_descended() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("tempdir");
    write(&tmp.path().join("real/inner.bin"), 50_000);
    write(&outside.path().join("big.bin"), 700_000);

    let inside_link = tmp.path().join("loop");
    let outside_link = tmp.path().join("escape");
    if !make_dir_link(&inside_link, &tmp.path().join("real"))
        || !make_dir_link(&outside_link, outside.path())
    {
        eprintln!("skipping: cannot create directory links here");
        return;
    }

    let tree = scan_ok(tmp.path());
    let expected = if cfg!(windows) {
        EntryKind::Junction
    } else {
        EntryKind::Symlink
    };
    for name in ["loop", "escape"] {
        let link = find(&tree, name);
        assert_eq!(link.kind, expected, "{name}");
        assert_eq!((link.logical, link.on_disk), (0, 0), "{name}");
        assert!(
            tree.entries.iter().all(|e| e.parent != Some(link.id)),
            "{name} must not be descended"
        );
    }
    assert_eq!(tree.stats.files, 1, "only real/inner.bin");
    assert_eq!(tree.stats.logical, 50_000, "no double count, no escape");
    let links = if cfg!(windows) {
        tree.stats.junctions
    } else {
        tree.stats.symlinks
    };
    assert_eq!(links, 2);
}

#[test]
fn file_symlinks_are_recorded_not_followed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("target.bin");
    write(&target, 9_000);
    let link = tmp.path().join("link.bin");
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&target, &link);
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&target, &link);
    if let Err(e) = made {
        eprintln!("skipping: no symlink privilege ({e})");
        return;
    }
    let tree = scan_ok(tmp.path());
    assert_eq!(find(&tree, "link.bin").kind, EntryKind::Symlink);
    assert_eq!(tree.stats.symlinks, 1);
    assert_eq!(tree.stats.logical, 9_000);
}

#[test]
fn junction_root_scans_its_target() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write(&tmp.path().join("real/f.bin"), 1234);
    let link = tmp.path().join("via");
    if !make_dir_link(&link, &tmp.path().join("real")) {
        eprintln!("skipping: cannot create directory links here");
        return;
    }
    let tree = scan_ok(&link);
    assert_eq!(tree.stats.files, 1);
    assert_eq!(tree.stats.logical, 1234);
}

#[cfg(unix)]
#[test]
fn unreadable_directory_is_an_issue_not_a_failure() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("tempdir");
    write(&tmp.path().join("ok.bin"), 10);
    let locked = tmp.path().join("locked");
    write(&locked.join("secret.bin"), 10);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    if fs::read_dir(&locked).is_ok() {
        eprintln!("skipping: running as root, permissions are not enforced");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod");
        return;
    }
    let tree = scan_ok(tmp.path());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod");

    assert_eq!(tree.stats.files, 1);
    assert_eq!(tree.stats.errors, 1);
    assert_eq!(tree.issues[0].kind, IssueKind::AccessDenied);
    assert!(tree.issues[0].path.ends_with("locked"));
}

#[test]
fn streams_progress_and_done() {
    let tmp = tempfile::tempdir().expect("tempdir");
    for i in 0..600 {
        write(&tmp.path().join(format!("d{}/f{i}.txt", i % 7)), 10);
    }
    let (tx, rx) = mpsc::channel();
    let options = ScanOptions {
        progress_interval: Duration::ZERO,
        threads: NonZeroUsize::new(2),
        ..ScanOptions::default()
    };
    let tree = scan(tmp.path(), &options, Some(&tx)).expect("scan");
    drop(tx);
    let events: Vec<ScanEvent> = rx.iter().collect();
    assert!(events.iter().any(|e| matches!(e, ScanEvent::Progress(_))));
    match events.last() {
        Some(ScanEvent::Done { stats, .. }) => assert_eq!(*stats, tree.stats),
        other => panic!("last event should be Done, got {other:?}"),
    }
    assert_eq!(tree.stats.files, 600);
}

#[test]
fn cancel_flag_aborts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    sample_tree(tmp.path());
    let options = ScanOptions {
        cancel: Some(Arc::new(AtomicBool::new(true))),
        ..ScanOptions::default()
    };
    assert!(matches!(
        scan(tmp.path(), &options, None),
        Err(ScanError::Cancelled)
    ));
}

#[test]
fn fast_mode_falls_back_to_standard() {
    let tmp = tempfile::tempdir().expect("tempdir");
    sample_tree(tmp.path());
    let options = ScanOptions {
        mode: ScanMode::Fast,
        ..ScanOptions::default()
    };
    let tree = scan(tmp.path(), &options, None).expect("scan");
    assert_eq!(tree.mode, ScanMode::Standard);
    assert!(tree.fallback.is_some());
    assert_eq!(tree.stats.files, 4);
}

#[test]
fn json_report_matches_documented_schema() {
    let tmp = tempfile::tempdir().expect("tempdir");
    sample_tree(tmp.path());
    let tree = scan_ok(tmp.path());
    let report = Report::new(&tree, 2, 0);
    let v = serde_json::to_value(&report).expect("serialize");

    let obj = v.as_object().expect("object");
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "children",
            "elapsed_ms",
            "errors",
            "extensions",
            "fallback",
            "min_size",
            "mode",
            "root",
            "scanned_at",
            "schema",
            "top",
            "top_files",
            "totals",
            "version"
        ]
    );
    assert_eq!(v["schema"], report::SCHEMA_ID);
    assert_eq!(v["mode"], "standard");
    assert!(v["fallback"].is_null());
    let mut totals: Vec<&str> = v["totals"]
        .as_object()
        .expect("totals")
        .keys()
        .map(String::as_str)
        .collect();
    totals.sort_unstable();
    assert_eq!(
        totals,
        [
            "approximate",
            "dirs",
            "errors",
            "files",
            "hardlink_dups",
            "junctions",
            "logical",
            "mount_points",
            "on_disk",
            "other",
            "symlinks"
        ]
    );
    assert_eq!(v["totals"]["logical"], 321_005);

    let child = &v["children"][0];
    assert_eq!(child["name"], "a");
    assert_eq!(child["kind"], "dir");
    assert_eq!(child["files"], 3);
    assert_eq!(v["children"].as_array().map(Vec::len), Some(2), "top=2");

    let file = &v["top_files"][0];
    for key in ["path", "logical", "on_disk", "mtime", "attrs", "hardlinks"] {
        assert!(file.get(key).is_some(), "top_files[0].{key}");
    }
    assert_eq!(file["logical"], 300_000);
    assert!(file["attrs"].is_array());
    assert_eq!(v["extensions"][0]["ext"], "bin");
    assert!(v["errors"].as_array().is_some_and(Vec::is_empty));
}

#[test]
fn csv_export_lists_entries() {
    let tmp = tempfile::tempdir().expect("tempdir");
    sample_tree(tmp.path());
    let tree = scan_ok(tmp.path());
    let mut out = Vec::new();
    report::write_csv(&tree, 0, &mut out).expect("csv");
    let text = String::from_utf8(out).expect("utf8");
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(report::CSV_HEADER));
    assert_eq!(lines.count(), tree.entries.len());
    assert!(text.contains(",file,300000,"));

    let mut big = Vec::new();
    report::write_csv(&tree, 100_000, &mut big).expect("csv");
    let rows = String::from_utf8(big).expect("utf8").lines().count() - 1;
    assert_eq!(rows, 5, "root, a, a/b, a/b/c, three.BIN");
}
