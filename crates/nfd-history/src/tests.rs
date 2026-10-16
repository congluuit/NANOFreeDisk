use std::path::{MAIN_SEPARATOR_STR, PathBuf};
use std::time::Duration;

use nfd_core::model::{Attrs, Entry, EntryId, EntryKind, roll_up};
use nfd_core::scanner::{ScanMode, ScanStats, ScanTree};

use super::*;

const MB: u64 = 1 << 20;
const GB: u64 = 1 << 30;
const DAY: i64 = 86_400;

use EntryKind::{Dir, File};

fn tree(root: &str, started_at: i64, spec: &[(Option<u32>, &str, EntryKind, u64)]) -> ScanTree {
    let mut entries: Vec<Entry> = spec
        .iter()
        .enumerate()
        .map(|(i, &(parent, name, kind, size))| Entry {
            id: EntryId(i as u32),
            parent: parent.map(EntryId),
            name: name.into(),
            kind,
            logical: size,
            on_disk: size,
            mtime: started_at - 1000,
            attrs: Attrs::default(),
            hardlink_count: 1,
            file_id: None,
            hardlink_dup: false,
            removed: false,
        })
        .collect();
    let file_counts = roll_up(&mut entries);
    ScanTree {
        root: PathBuf::from(root),
        stats: ScanStats {
            files: file_counts[0],
            dirs: entries.iter().filter(|e| e.kind == Dir).count() as u64,
            logical: entries[0].logical,
            on_disk: entries[0].on_disk,
            ..ScanStats::default()
        },
        entries,
        file_counts,
        issues: Vec::new(),
        mode: ScanMode::Standard,
        fallback: None,
        started_at,
        elapsed: Duration::from_millis(1500),
    }
}

fn p(parts: &[&str]) -> String {
    parts.join(MAIN_SEPARATOR_STR)
}

/// Week 1: games/steam/common/a.pak (20 GB), old/ (2 GB), docs/small.txt.
fn week1() -> ScanTree {
    tree(
        r"D:\Data",
        1_000 * DAY,
        &[
            (None, "Data", Dir, 0),
            (Some(0), "games", Dir, 0),
            (Some(1), "steam", Dir, 0),
            (Some(2), "common", Dir, 0),
            (Some(3), "a.pak", File, 20 * GB),
            (Some(0), "old", Dir, 0),
            (Some(5), "backup.zip", File, 2 * GB),
            (Some(0), "docs", Dir, 0),
            (Some(7), "small.txt", File, 4096),
        ],
    )
}

/// Week 2: steam/common grew by 5 GB (new b.pak), old/ is gone, a new 3 GB video,
/// a new tiny file.
fn week2() -> ScanTree {
    tree(
        r"D:\Data\",
        1_007 * DAY,
        &[
            (None, "Data", Dir, 0),
            (Some(0), "games", Dir, 0),
            (Some(1), "steam", Dir, 0),
            (Some(2), "common", Dir, 0),
            (Some(3), "a.pak", File, 20 * GB),
            (Some(3), "b.pak", File, 5 * GB),
            (Some(0), "docs", Dir, 0),
            (Some(6), "small.txt", File, 4096),
            (Some(6), "tiny-new.txt", File, 100),
            (Some(0), "video.mp4", File, 3 * GB),
        ],
    )
}

fn vol(free: u64) -> VolumeState {
    VolumeState {
        total: 1000 * GB,
        free,
    }
}

#[test]
fn record_list_and_retention() {
    let mut h = History::open_in_memory().expect("open");
    let o = RecordOptions {
        keep: 2,
        ..RecordOptions::default()
    };
    let first = h
        .record(&week1(), r"D:\", vol(500 * GB), &o)
        .expect("record");
    let second = h
        .record(&week2(), r"D:\", vol(492 * GB), &o)
        .expect("record");
    let snaps = h.snapshots(r"d:\data").expect("list");
    assert_eq!(
        snaps.iter().map(|s| s.id).collect::<Vec<_>>(),
        [second, first]
    );
    let s = &snaps[1];
    assert_eq!(s.root, r"D:\Data");
    assert_eq!(s.volume, r"d:\");
    assert_eq!(s.files, 3);
    assert_eq!(s.on_disk, 22 * GB + 4096);
    assert_eq!((s.volume_total, s.volume_free), (1000 * GB, 500 * GB));
    assert_eq!(s.elapsed_ms, 1500);
    assert_eq!(s.dir_floor, MB);
    assert_eq!(s.file_floor, 10 * MB);

    // a third snapshot pushes the first one out (keep = 2), dirs/files go with it
    let mut later = week2();
    later.started_at += DAY;
    let third = h.record(&later, r"D:\", vol(490 * GB), &o).expect("record");
    let ids: Vec<i64> = h
        .snapshots(r"D:\Data")
        .expect("list")
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, [third, second]);
    assert!(matches!(h.snapshot(first), Err(HistoryError::NotFound(_))));
    let orphans: i64 = h
        .conn
        .query_row(
            "SELECT COUNT(*) FROM dirs WHERE scan_id = ?1",
            [first],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(orphans, 0);
    assert_eq!(h.count().expect("count"), 2);
    h.clear().expect("clear");
    assert_eq!(h.count().expect("count"), 0);
}

#[test]
fn total_cap_and_forget() {
    let mut h = History::open_in_memory().expect("open");
    let o = RecordOptions {
        keep_total: 3,
        ..RecordOptions::default()
    };
    for i in 0..5 {
        let t = tree(&format!(r"C:\tmp\f{i}"), i * DAY, &[(None, "x", Dir, 0)]);
        h.record(&t, "", VolumeState::default(), &o)
            .expect("record");
    }
    assert_eq!(h.count().expect("count"), 3);
    assert!(
        h.snapshots(r"C:\tmp\f0").expect("list").is_empty(),
        "oldest went first"
    );
    assert_eq!(h.forget(r"C:\TMP\F4\").expect("forget"), 1);
    assert_eq!(h.forget(r"C:\tmp\f4").expect("forget"), 0);
    assert_eq!(h.count().expect("count"), 2);
}

#[test]
fn caps_raise_the_floors() {
    let mut h = History::open_in_memory().expect("open");
    let o = RecordOptions {
        max_dirs: 3,
        max_files: 2,
        top_files: 0,
        ..RecordOptions::default()
    };
    let id = h
        .record(&week2(), "", VolumeState::default(), &o)
        .expect("record");
    let s = h.snapshot(id).expect("snapshot");
    // dirs by size: Data 28G, games 25G, steam 25G, common 25G, docs 4K → 3 kept, the
    // largest dropped one is 25 GB
    assert_eq!(s.dir_floor, 25 * GB + 1);
    // files: a 20G, b 5G, video 3G → 2 kept
    assert_eq!(s.file_floor, 3 * GB + 1);
    assert_eq!(s.volume, "");
}

#[test]
fn diff_reports_specific_folders_and_large_files() {
    let mut h = History::open_in_memory().expect("open");
    let o = RecordOptions::default();
    let a = h
        .record(&week1(), r"D:\", vol(500 * GB), &o)
        .expect("record");
    let b = h
        .record(&week2(), r"D:\", vol(492 * GB), &o)
        .expect("record");
    let d = h.diff(a, b, &DiffOptions::default()).expect("diff");

    assert_eq!(
        d.on_disk_delta,
        to_i64(3 * GB + 5 * GB) - to_i64(2 * GB) + 100
    );
    assert_eq!(d.files_delta, 2);
    assert_eq!(d.free_delta, Some(-(8 * GB as i64)));
    // games/ and steam/ grew only because of common/: just the specific folder
    let grew: Vec<(&str, i64)> = d.grew.iter().map(|c| (c.path.as_str(), c.delta)).collect();
    assert_eq!(
        grew,
        [(p(&["games", "steam", "common"]).as_str(), to_i64(5 * GB))]
    );
    assert_eq!(d.grew[0].files_delta, 1);
    assert_eq!(d.grew[0].depth, 3);
    let shrank: Vec<(&str, u64, u64)> = d
        .shrank
        .iter()
        .map(|c| (c.path.as_str(), c.before, c.after))
        .collect();
    assert_eq!(shrank, [("old", 2 * GB, 0)]);

    let up: Vec<(&str, u64)> = d
        .files_up
        .iter()
        .map(|f| (f.path.as_str(), f.before))
        .collect();
    let b_pak = p(&["games", "steam", "common", "b.pak"]);
    assert_eq!(up, [(b_pak.as_str(), 0), ("video.mp4", 0)]);
    let down: Vec<(&str, u64)> = d
        .files_down
        .iter()
        .map(|f| (f.path.as_str(), f.after))
        .collect();
    assert_eq!(down, [(p(&["old", "backup.zip"]).as_str(), 0)]);
    // tiny-new.txt is recorded (top files) but too small to have been recorded
    // before, so it is not claimed as new; small.txt did not change
    assert!(!d.files_up.iter().any(|f| f.path.contains("tiny")));

    // reverse direction mirrors it
    let r = h.diff(b, a, &DiffOptions::default()).expect("diff");
    assert_eq!(r.shrank[0].path, p(&["games", "steam", "common"]));
    assert_eq!(r.grew[0].path, "old");
    assert!(matches!(
        h.diff(a, 999, &DiffOptions::default()),
        Err(HistoryError::NotFound(999))
    ));
}

#[test]
fn growth_split_over_two_children_keeps_the_parent() {
    let mut h = History::open_in_memory().expect("open");
    let o = RecordOptions::default();
    let before = tree(
        r"C:\x",
        0,
        &[
            (None, "x", Dir, 0),
            (Some(0), "p", Dir, 0),
            (Some(1), "a", Dir, 0),
            (Some(1), "b", Dir, 0),
        ],
    );
    let after = tree(
        r"C:\x",
        DAY,
        &[
            (None, "x", Dir, 0),
            (Some(0), "p", Dir, 0),
            (Some(1), "a", Dir, 0),
            (Some(2), "f1", File, GB),
            (Some(1), "b", Dir, 0),
            (Some(4), "f2", File, GB),
        ],
    );
    let a = h
        .record(&before, "", VolumeState::default(), &o)
        .expect("record");
    let b = h
        .record(&after, "", VolumeState::default(), &o)
        .expect("record");
    let d = h.diff(a, b, &DiffOptions::default()).expect("diff");
    let grew: Vec<&str> = d.grew.iter().map(|c| c.path.as_str()).collect();
    // p grew 2 GB, no single child explains 80 % of it: p, a and b are all listed
    assert_eq!(grew, ["p", &p(&["p", "a"]), &p(&["p", "b"])]);
    assert_eq!(d.free_delta, None, "volume unknown");
}

#[test]
fn forecast_fits_free_space() {
    let mut h = History::open_in_memory().expect("open");
    let o = RecordOptions::default();
    let mut t = week1();
    // 10 GB less free space per day over 3 days
    for (day, free) in [(0, 500), (1, 490), (2, 480)] {
        t.started_at = 1_000 * DAY + day * DAY;
        h.record(&t, r"D:\", vol(free * GB), &o).expect("record");
    }
    let now = 1_003 * DAY;
    let f = h
        .forecast(r"D:\", now, 470 * GB, 1000 * GB, 90)
        .expect("forecast")
        .expect("enough points");
    assert_eq!(f.samples, 4);
    assert!((f.bytes_per_day + 10.0 * GB as f64).abs() < 1.0, "{f:?}");
    assert!((f.days_to_full.expect("filling") - 47.0).abs() < 0.01);
    assert!((f.span_days - 3.0).abs() < 1e-9);
    // other volume, or too few points: no forecast
    assert!(h.forecast(r"C:\", now, GB, GB, 90).expect("ok").is_none());
    assert!(
        h.forecast(r"D:\", now, 470 * GB, 1000 * GB, 1)
            .expect("ok")
            .is_none()
    );
    // freeing space: no "days to full"
    let f = h
        .forecast(r"D:\", now, 600 * GB, 1000 * GB, 90)
        .expect("ok")
        .expect("points");
    assert!(f.days_to_full.is_none());
}

#[test]
fn damaged_file_is_set_aside() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sub").join("history.db");
    {
        let mut h = History::open(&path).expect("create");
        h.record(
            &week1(),
            "",
            VolumeState::default(),
            &RecordOptions::default(),
        )
        .expect("record");
    }
    // reopen keeps the data
    assert_eq!(
        History::open(&path)
            .expect("reopen")
            .count()
            .expect("count"),
        1
    );
    fs::write(
        &path,
        b"this is not a database, just garbage bytes ...........",
    )
    .expect("write");
    let h = History::open(&path).expect("recreated");
    assert_eq!(h.count().expect("count"), 0);
    let aside = fs::read_dir(path.parent().expect("dir"))
        .expect("list")
        .filter_map(|e| e.ok())
        .any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("history.db.broken-")
        });
    assert!(aside, "the damaged file is kept for inspection");
}

#[test]
fn newer_schema_is_left_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history.db");
    {
        let conn = Connection::open(&path).expect("open");
        conn.execute_batch("PRAGMA user_version = 99;")
            .expect("version");
    }
    assert!(matches!(History::open(&path), Err(HistoryError::Newer(99))));
    assert!(path.exists());
}

#[test]
fn root_keys() {
    assert_eq!(root_key(r"C:\"), r"c:\");
    assert_eq!(root_key("C:"), r"c:\");
    assert_eq!(root_key(r"D:\Data\"), r"d:\data");
    assert_eq!(root_key("/home/x/"), "/home/x");
}

/// `cargo test -p nfd-history --release -- --ignored --nocapture`: time to store a
/// snapshot of a 1M-entry tree (100k folders × 9 files).
#[test]
#[ignore = "benchmark"]
fn record_a_million_entries() {
    let mut spec: Vec<(Option<u32>, String, EntryKind, u64)> = vec![(None, "root".into(), Dir, 0)];
    for d in 0..100_000u32 {
        let dir = spec.len() as u32;
        let parent = if d < 100 { 0 } else { 1 + (d % 100) * 10 };
        spec.push((Some(parent.min(dir - 1)), format!("d{d}"), Dir, 0));
        for f in 0..9u64 {
            spec.push((
                Some(dir),
                format!("f{f}.bin"),
                File,
                (u64::from(d) % 97 + 1) * (f + 1) * 40_000,
            ));
        }
    }
    let spec: Vec<(Option<u32>, &str, EntryKind, u64)> = spec
        .iter()
        .map(|(p, n, k, s)| (*p, n.as_str(), *k, *s))
        .collect();
    let t = tree(r"C:\big", 0, &spec);
    let dir = tempfile::tempdir().expect("tempdir");
    let mut h = History::open(&dir.path().join("h.db")).expect("open");
    let start = std::time::Instant::now();
    let id = h
        .record(&t, r"C:\", vol(GB), &RecordOptions::default())
        .expect("record");
    let took = start.elapsed();
    let s = h.snapshot(id).expect("snapshot");
    println!(
        "recorded {} entries in {took:?} (dir floor {}, file floor {})",
        t.entries.len(),
        s.dir_floor,
        s.file_floor
    );
    assert!(took < Duration::from_secs(5));
}
