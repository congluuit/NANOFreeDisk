//! Light benchmark of the duplicate pipeline: `cargo bench -p nfd-dupes`.
//!
//! Builds a small synthetic set in a temporary folder (by default 96 files of 1 MiB,
//! a third of them duplicating another, all of the same size) and times a cold run and
//! a run answered by the hash cache. Short on purpose — real-disk numbers for the
//! 100 GB goal are in `docs/dupes-benchmark.md`. `NFD_BENCH_FILES` and
//! `NFD_BENCH_SIZE` (bytes) change the set.
//!
//! `NFD_BENCH_DIR=<folder>` measures a real folder instead: the scan, a cold and a
//! cached pipeline run, and the full-hash throughput over **every** file of at least
//! 1 MiB (what the first run costs when every file has a same-size twin — the worst
//! case the 100 GB goal is extrapolated from).

use std::fs;
use std::time::Instant;

use nfd_core::safety::SafetyGate;
use nfd_core::scanner::{self, ScanOptions};
use nfd_dupes::cache::MemoryCache;
use nfd_dupes::hash::LiveSource;
use nfd_dupes::{DupeOptions, find};

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    if let Some(dir) = std::env::var_os("NFD_BENCH_DIR") {
        real(std::path::Path::new(&dir));
        return;
    }
    let files = env("NFD_BENCH_FILES", 96);
    let size = env("NFD_BENCH_SIZE", 1 << 20) as usize;
    let dir = tempfile::tempdir().expect("tmp");
    let mut data = vec![0u8; size];
    for n in 0..files {
        // every third file repeats the previous one; the others are all different
        let seed = if n % 3 == 2 { n - 1 } else { n };
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i as u64).wrapping_mul(2_654_435_761).wrapping_add(seed) as u8;
        }
        fs::write(dir.path().join(format!("f{n:04}.bin")), &data).expect("write");
    }
    let total = files * size as u64;
    let tree = scanner::scan(dir.path(), &ScanOptions::default(), None).expect("scan");
    let gate = SafetyGate::new(&|_: &str| None);
    let opts = DupeOptions {
        min_size: 1,
        ..DupeOptions::default()
    };
    let mut cache = MemoryCache::default();
    for label in ["cold", "cached"] {
        let t = Instant::now();
        let r = find(
            &[&tree],
            &gate,
            &opts,
            Some(&mut cache),
            &LiveSource,
            &|_| {},
        )
        .expect("find");
        let s = t.elapsed().as_secs_f64();
        println!(
            "{label:>6}: {files} files, {:.0} MiB in {s:.3} s ({:.0} MiB/s read) — {} groups, {} MiB read, {} cache hits",
            total as f64 / 1_048_576.0,
            r.hashed_bytes as f64 / 1_048_576.0 / s.max(1e-9),
            r.groups.len(),
            r.hashed_bytes >> 20,
            r.cache_hits
        );
    }
}

fn real(root: &std::path::Path) {
    use rayon::prelude::*;
    use std::sync::atomic::AtomicU64;

    let t = Instant::now();
    let tree = scanner::scan(root, &ScanOptions::default(), None).expect("scan");
    println!(
        "  scan: {} files, {:.2} GiB in {:.2} s",
        tree.stats.files,
        tree.stats.logical as f64 / (1u64 << 30) as f64,
        t.elapsed().as_secs_f64()
    );
    let gate = SafetyGate::new(&|_: &str| None);
    let opts = DupeOptions::default();
    let mut cache = MemoryCache::default();
    for label in ["cold", "cached"] {
        let t = Instant::now();
        let r = find(
            &[&tree],
            &gate,
            &opts,
            Some(&mut cache),
            &LiveSource,
            &|_| {},
        )
        .expect("find");
        println!(
            "{label:>6}: {} candidates, {} groups, {:.1} MiB wasted, read {:.1} MiB, {} cache hits in {:.2} s",
            r.candidates,
            r.groups.len(),
            r.wasted_bytes as f64 / 1_048_576.0,
            r.hashed_bytes as f64 / 1_048_576.0,
            r.cache_hits,
            t.elapsed().as_secs_f64()
        );
    }
    // worst case: every file >= 1 MiB read whole, 4 at a time, background priority
    let files: Vec<(std::path::PathBuf, u64, i64)> = tree
        .entries
        .iter()
        .filter(|e| e.kind == nfd_core::model::EntryKind::File && e.logical >= 1 << 20)
        .map(|e| (tree.path(e.id), e.logical, e.mtime))
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(nfd_dupes::DEFAULT_THREADS)
        .start_handler(|_| nfd_platform::io::background_priority())
        .build()
        .expect("pool");
    let ctx = nfd_dupes::hash::Ctx::default();
    let bytes = AtomicU64::new(0);
    let t = Instant::now();
    pool.install(|| {
        files.par_iter().for_each(|(p, size, mtime)| {
            if nfd_dupes::hash::full(&LiveSource, p, *size, *mtime, &ctx).is_ok() {
                bytes.fetch_add(*size, std::sync::atomic::Ordering::Relaxed);
            }
        });
    });
    let s = t.elapsed().as_secs_f64();
    let b = bytes.into_inner() as f64;
    let rate = b / s.max(1e-9);
    println!(
        "  full hash of every file >= 1 MiB: {} files, {:.2} GiB in {s:.2} s = {:.0} MiB/s -> 100 GB in {:.0} s",
        files.len(),
        b / (1u64 << 30) as f64,
        rate / 1_048_576.0,
        100e9 / rate.max(1.0)
    );
}
