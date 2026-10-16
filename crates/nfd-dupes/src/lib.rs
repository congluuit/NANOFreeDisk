//! Duplicates (Phase 5, `docs/PHASE5_DUPLICATES.md`): files with exactly the same
//! bytes, found without ever reporting a false match.
//!
//! The pipeline narrows the candidates step by step, each step cheaper than the next:
//!
//! 1. **[`collect`]** — files of one or more scans, minus empty and small files,
//!    cloud placeholders, reparse points, SYSTEM files, protected locations (the
//!    SafetyGate, AI conversation history included) and application folders.
//! 2. **Size** — only sizes seen at least twice; several names of one file (hard
//!    links, same volume and file id) count as one copy, never as a duplicate of
//!    itself.
//! 3. **Partial hash** — BLAKE3 of 16 KiB at the head, the middle and the tail.
//! 4. **Full hash** — BLAKE3 of the whole file (1 MiB reads, no memory mapping),
//!    several files at a time on SSDs and one at a time on spinning disks, threads
//!    in Windows background mode (low CPU and I/O priority).
//! 5. **Paranoid** (optional) — byte-by-byte comparison within each group.
//!
//! Hashes are cached in the history database ([`cache`]) by volume, file id, size
//! and last write, so a second run reads almost nothing.
//!
//! Nothing here deletes anything: [`keep`] picks the copy that stays, [`plan`]
//! turns a selection into a `CleanPlan` for the Phase 3 path (SafetyGate → Executor
//! → UndoJournal) and checks keeper and copy again right before each item.

pub mod cache;
pub mod collect;
pub mod hash;
pub mod keep;
pub mod plan;

use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use nfd_core::safety::SafetyGate;
use nfd_core::scanner::ScanTree;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cache::{CacheKey, CachedHash, HashCache};
use crate::collect::{Cand, Skips};
use crate::hash::{Ctx, Refused, SAMPLE, Source};

/// JSON schema id of `nfd dupes --json`.
pub const SCHEMA: &str = "nanofreedisk.dupes.v1";
/// Files smaller than this are not considered by default (1 MiB).
pub const DEFAULT_MIN_SIZE: u64 = 1 << 20;
/// Files hashed at once on an SSD / NVMe drive by default.
pub const DEFAULT_THREADS: usize = 4;

/// What to look for.
#[derive(Debug, Clone)]
pub struct DupeOptions {
    /// Smaller files are left out (empty files never count).
    pub min_size: u64,
    /// Also look inside application folders (their copies are only shown, never
    /// pre-selected).
    pub include_app_dirs: bool,
    /// Compare groups byte by byte after the full hash.
    pub paranoid: bool,
    /// Files hashed at once per SSD (spinning disks: one).
    pub threads: Option<NonZeroUsize>,
    /// Only these extensions (lower case, no dot); empty = all.
    pub extensions: Vec<String>,
    /// Application folders by location ([`collect::default_app_dirs`]).
    pub app_dir_paths: Vec<PathBuf>,
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Default for DupeOptions {
    fn default() -> Self {
        Self {
            min_size: DEFAULT_MIN_SIZE,
            include_app_dirs: false,
            paranoid: false,
            threads: None,
            extensions: Vec::new(),
            app_dir_paths: Vec::new(),
            cancel: None,
        }
    }
}

/// Pipeline step, for progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Collect,
    Partial,
    Full,
    Compare,
}

/// Progress of one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Phase,
    /// Files done / to do in this step.
    pub done: u64,
    pub total: u64,
    /// Bytes read so far (all steps).
    pub bytes: u64,
    /// Bytes this step reads at most.
    pub total_bytes: u64,
}

/// Why a file left the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    Empty,
    Small,
    Extension,
    /// Cloud placeholder: never opened, never read.
    Cloud,
    Reparse,
    System,
    /// Refused by the SafetyGate (system, program, credential, AI history…).
    Protected,
    AppDir,
    Unreadable,
    /// Size or last write changed since the scan (or while it was read).
    Changed,
}

impl SkipReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Small => "small",
            Self::Extension => "extension",
            Self::Cloud => "cloud",
            Self::Reparse => "reparse",
            Self::System => "system",
            Self::Protected => "protected",
            Self::AppDir => "app-dir",
            Self::Unreadable => "unreadable",
            Self::Changed => "changed",
        }
    }
}

/// One file left out, as an example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: SkipReason,
}

/// Facts about where a copy lives (warnings and default selection).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyFlags {
    /// Inside a OneDrive / Dropbox / Google Drive… folder: deleting it there
    /// deletes it in the cloud and on every synced PC.
    pub cloud: bool,
    /// Inside a git working tree (may be a tracked file).
    pub git: bool,
    /// Inside an application folder (shown only with `include_app_dirs`).
    pub app_dir: bool,
    pub readonly: bool,
}

/// How a copy can be replaced by a link to the copy that stays (same volume only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkKind {
    /// NTFS hard link: one file, two names (shared contents, attributes, times).
    Hardlink,
    /// ReFS / Dev Drive block clone: copy-on-write, the files stay independent.
    Clone,
}

/// One copy of the duplicated contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCopy {
    pub path: PathBuf,
    /// Last write, Unix seconds.
    pub mtime: i64,
    /// Volume serial (as text: it may not fit a JSON number).
    #[serde(with = "hexnum::u64")]
    pub volume: u64,
    #[serde(with = "hexnum::opt_u128")]
    pub file_id: Option<u128>,
    /// Other names of the same file (hard links seen in the scan).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<PathBuf>,
    pub flags: CopyFlags,
    /// How its volume can replace a duplicate by a link (`None`: FAT, network…).
    #[serde(default)]
    pub link: Option<LinkKind>,
}

/// Files with identical contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeGroup {
    /// 1-based, in report order.
    pub id: u32,
    /// BLAKE3 of the contents, hex.
    pub hash: String,
    /// Size of each copy.
    pub size: u64,
    /// `size × (copies − 1)`: what keeping one copy frees.
    pub wasted: u64,
    pub copies: Vec<FileCopy>,
}

/// Result of a run.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeReport {
    pub roots: Vec<PathBuf>,
    pub min_size: u64,
    /// Largest waste first.
    pub groups: Vec<DupeGroup>,
    pub wasted_bytes: u64,
    /// Files looked at in the scans.
    pub scanned_files: u64,
    /// Files that shared their size with another one.
    pub candidates: u64,
    /// Bytes read from disk.
    pub hashed_bytes: u64,
    /// Files whose hashes came from the cache.
    pub cache_hits: u64,
    pub skipped: BTreeMap<SkipReason, u64>,
    pub skipped_samples: Vec<Skipped>,
    pub elapsed_ms: u64,
    pub paranoid: bool,
}

/// The run was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("duplicate search cancelled")]
pub struct Cancelled;

/// Distinct contents holder: one file (possibly several hard-linked names).
#[derive(Debug, Clone)]
struct Unit {
    cand: usize,
    links: Vec<usize>,
    partial: Option<[u8; 32]>,
    full: Option<[u8; 32]>,
    /// Hashes computed in this run (to store in the cache).
    fresh: bool,
    gone: bool,
}

/// Find duplicates among the files of `trees`.
///
/// `progress` is called from worker threads at most every 100 ms per step (and
/// once at the end of each step).
///
/// # Errors
/// [`Cancelled`] when `opts.cancel` was raised.
pub fn find(
    trees: &[&ScanTree],
    gate: &SafetyGate,
    opts: &DupeOptions,
    cache: Option<&mut dyn HashCache>,
    source: &dyn Source,
    progress: &(dyn Fn(&Progress) + Sync),
) -> Result<DupeReport, Cancelled> {
    let started = Instant::now();
    let ctx = Ctx {
        cancel: opts.cancel.as_deref(),
        bytes: AtomicU64::new(0),
    };
    let mut skips = Skips::default();
    progress(&Progress {
        phase: Phase::Collect,
        done: 0,
        total: 0,
        bytes: 0,
        total_bytes: 0,
    });
    let (cands, scanned) = collect::collect(trees, gate, opts, source, &mut skips);
    if ctx.cancelled() {
        return Err(Cancelled);
    }

    // 2. by size; hard-linked names of one file become one unit
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, c) in cands.iter().enumerate() {
        by_size.entry(c.size).or_default().push(i);
    }
    let mut units: Vec<Unit> = Vec::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for idxs in by_size.into_values().filter(|v| v.len() > 1) {
        let mut seen: HashMap<(u64, u128), usize> = HashMap::new();
        let mut group = Vec::new();
        for i in idxs {
            let c = &cands[i];
            if let Some(id) = c.file_id {
                if let Some(&u) = seen.get(&(c.volume, id)) {
                    units[u].links.push(i);
                    continue;
                }
                seen.insert((c.volume, id), units.len());
            }
            group.push(units.len());
            units.push(Unit {
                cand: i,
                links: Vec::new(),
                partial: None,
                full: None,
                fresh: false,
                gone: false,
            });
        }
        if group.len() > 1 {
            groups.push(group);
        }
    }
    let candidates: u64 = groups.iter().map(|g| g.len() as u64).sum();

    // cached hashes
    let mut cache_hits = 0u64;
    if let Some(cache) = cache.as_deref() {
        let wanted: Vec<(usize, CacheKey)> = groups
            .iter()
            .flatten()
            .filter_map(|&u| cache_key(&cands[units[u].cand]).map(|k| (u, k)))
            .collect();
        let keys: Vec<CacheKey> = wanted.iter().map(|(_, k)| *k).collect();
        for ((u, _), hit) in wanted.iter().zip(cache.get(&keys)) {
            if let Some(hit) = hit {
                units[*u].partial = hit.partial;
                units[*u].full = hit.full;
                cache_hits += 1;
            }
        }
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads.map_or(DEFAULT_THREADS, NonZeroUsize::get))
        .thread_name(|i| format!("nfd-dupes-{i}"))
        .start_handler(|_| nfd_platform::io::background_priority())
        .build()
        .ok();
    let mut hdd: HashMap<u64, bool> = HashMap::new();
    for &u in groups.iter().flatten() {
        let c = &cands[units[u].cand];
        hdd.entry(c.volume)
            .or_insert_with(|| source.seek_penalty(&c.path) == Some(true));
    }

    // 3. partial hash
    let todo: Vec<usize> = groups
        .iter()
        .flatten()
        .copied()
        .filter(|&u| units[u].partial.is_none())
        .collect();
    let total_bytes = todo
        .iter()
        .map(|&u| cands[units[u].cand].size.min(3 * SAMPLE))
        .sum();
    let results = run(
        pool.as_ref(),
        &todo,
        &cands,
        &units,
        &hdd,
        Phase::Partial,
        total_bytes,
        &ctx,
        progress,
        |c| hash::partial(source, &c.path, c.size, c.mtime, &ctx),
    );
    for (u, r) in todo.iter().zip(results) {
        match r {
            Ok((digest, whole)) => {
                units[*u].partial = Some(digest);
                if whole {
                    units[*u].full = Some(digest);
                }
                units[*u].fresh = true;
            }
            Err(Refused::Cancelled) => return Err(Cancelled),
            Err(Refused::Skip(reason)) => {
                units[*u].gone = true;
                skips.add(reason, || cands[units[*u].cand].path.clone());
            }
        }
    }
    let groups = regroup(&groups, &units, |u| u.partial);

    // 4. full hash
    let todo: Vec<usize> = groups
        .iter()
        .flatten()
        .copied()
        .filter(|&u| units[u].full.is_none())
        .collect();
    let total_bytes = todo.iter().map(|&u| cands[units[u].cand].size).sum();
    let results = run(
        pool.as_ref(),
        &todo,
        &cands,
        &units,
        &hdd,
        Phase::Full,
        total_bytes,
        &ctx,
        progress,
        |c| hash::full(source, &c.path, c.size, c.mtime, &ctx),
    );
    for (u, r) in todo.iter().zip(results) {
        match r {
            Ok(digest) => {
                units[*u].full = Some(digest);
                units[*u].fresh = true;
            }
            Err(Refused::Cancelled) => return Err(Cancelled),
            Err(Refused::Skip(reason)) => {
                units[*u].gone = true;
                skips.add(reason, || cands[units[*u].cand].path.clone());
            }
        }
    }
    let mut groups = regroup(&groups, &units, |u| u.full);

    // remember what was read
    if let Some(cache) = cache {
        let rows: Vec<(CacheKey, CachedHash)> = units
            .iter()
            .filter(|u| u.fresh && !u.gone)
            .filter_map(|u| {
                cache_key(&cands[u.cand]).map(|k| {
                    (
                        k,
                        CachedHash {
                            partial: u.partial,
                            full: u.full,
                        },
                    )
                })
            })
            .collect();
        cache.put(&rows);
    }

    // 5. paranoid: byte by byte against the first copy of each group
    if opts.paranoid {
        let total: u64 = groups.iter().map(|g| g.len() as u64 - 1).sum();
        let done = AtomicU64::new(0);
        let mut split = Vec::new();
        for g in &groups {
            let first = &cands[units[g[0]].cand];
            let mut same = vec![g[0]];
            let mut other = Vec::new();
            for &u in &g[1..] {
                let c = &cands[units[u].cand];
                match hash::same_bytes(
                    source,
                    (&first.path, first.size, first.mtime),
                    (&c.path, c.size, c.mtime),
                    &ctx,
                ) {
                    Ok(true) => same.push(u),
                    Ok(false) => other.push(u),
                    Err(Refused::Cancelled) => return Err(Cancelled),
                    Err(Refused::Skip(reason)) => skips.add(reason, || c.path.clone()),
                }
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(&Progress {
                    phase: Phase::Compare,
                    done: d,
                    total,
                    bytes: ctx.bytes.load(Ordering::Relaxed),
                    total_bytes: 0,
                });
            }
            if same.len() > 1 {
                split.push(same);
            }
            // a mismatch would be a hash collision: never seen, but never merged either
            if other.len() > 1 {
                split.push(other);
            }
        }
        groups = split;
    }

    // report
    let mut link_modes: HashMap<u64, Option<LinkKind>> = HashMap::new();
    for &u in groups.iter().flatten() {
        let c = &cands[units[u].cand];
        link_modes
            .entry(c.volume)
            .or_insert_with(|| source.link_mode(&c.path));
    }
    let mut out: Vec<DupeGroup> = groups
        .iter()
        .map(|g| {
            let first = &cands[units[g[0]].cand];
            let mut copies: Vec<FileCopy> = g
                .iter()
                .map(|&u| {
                    let c = &cands[units[u].cand];
                    let mut links: Vec<PathBuf> = units[u]
                        .links
                        .iter()
                        .map(|&l| cands[l].path.clone())
                        .collect();
                    links.sort();
                    FileCopy {
                        path: c.path.clone(),
                        mtime: c.mtime,
                        volume: c.volume,
                        file_id: c.file_id,
                        links,
                        flags: c.flags,
                        link: link_modes.get(&c.volume).copied().flatten(),
                    }
                })
                .collect();
            copies.sort_by(|a, b| a.path.cmp(&b.path));
            DupeGroup {
                id: 0,
                hash: units[g[0]].full.map(|d| hash::hex(&d)).unwrap_or_default(),
                size: first.size,
                wasted: first.size * (copies.len() as u64 - 1),
                copies,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.wasted
            .cmp(&a.wasted)
            .then(b.size.cmp(&a.size))
            .then_with(|| a.copies[0].path.cmp(&b.copies[0].path))
    });
    for (n, g) in out.iter_mut().enumerate() {
        g.id = u32::try_from(n + 1).unwrap_or(u32::MAX);
    }
    Ok(DupeReport {
        roots: trees.iter().map(|t| t.root.clone()).collect(),
        min_size: opts.min_size,
        wasted_bytes: out.iter().map(|g| g.wasted).sum(),
        groups: out,
        scanned_files: scanned,
        candidates,
        hashed_bytes: ctx.bytes.load(Ordering::Relaxed),
        cache_hits,
        skipped: skips.counts,
        skipped_samples: skips.samples,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        paranoid: opts.paranoid,
    })
}

fn cache_key(c: &Cand) -> Option<CacheKey> {
    Some(CacheKey {
        volume: c.volume,
        file_id: c.file_id?,
        size: c.size,
        mtime: c.mtime,
    })
}

/// Split every group by `digest`, dropping units that left and singletons.
fn regroup(
    groups: &[Vec<usize>],
    units: &[Unit],
    digest: impl Fn(&Unit) -> Option<[u8; 32]>,
) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    for g in groups {
        let mut by: HashMap<[u8; 32], Vec<usize>> = HashMap::new();
        for &u in g {
            if units[u].gone {
                continue;
            }
            if let Some(d) = digest(&units[u]) {
                by.entry(d).or_default().push(u);
            }
        }
        out.extend(by.into_values().filter(|v| v.len() > 1));
    }
    out
}

/// Run `f` over the units `todo` (result order = `todo` order): spinning disks one
/// file at a time in path order, other volumes in parallel.
#[allow(clippy::too_many_arguments)]
fn run<R: Send>(
    pool: Option<&rayon::ThreadPool>,
    todo: &[usize],
    cands: &[Cand],
    units: &[Unit],
    hdd: &HashMap<u64, bool>,
    phase: Phase,
    total_bytes: u64,
    ctx: &Ctx<'_>,
    progress: &(dyn Fn(&Progress) + Sync),
    f: impl Fn(&Cand) -> R + Sync,
) -> Vec<R> {
    let total = todo.len() as u64;
    let done = AtomicU64::new(0);
    let last = Mutex::new(Instant::now());
    let tick = || {
        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
        let due = last.lock().is_ok_and(|mut t| {
            if t.elapsed() >= Duration::from_millis(100) || d == total {
                *t = Instant::now();
                true
            } else {
                false
            }
        });
        if due {
            progress(&Progress {
                phase,
                done: d,
                total,
                bytes: ctx.bytes.load(Ordering::Relaxed),
                total_bytes,
            });
        }
    };
    // per volume, in path order
    let mut by_volume: BTreeMap<u64, Vec<(usize, usize)>> = BTreeMap::new();
    for (pos, &u) in todo.iter().enumerate() {
        by_volume
            .entry(cands[units[u].cand].volume)
            .or_default()
            .push((pos, u));
    }
    for jobs in by_volume.values_mut() {
        jobs.sort_by(|a, b| {
            cands[units[a.1].cand]
                .path
                .cmp(&cands[units[b.1].cand].path)
        });
    }
    let job = |&(pos, u): &(usize, usize)| {
        let r = f(&cands[units[u].cand]);
        tick();
        (pos, r)
    };
    let work = || -> Vec<(usize, R)> {
        by_volume
            .par_iter()
            .flat_map_iter(|(vol, jobs)| {
                if hdd.get(vol).copied().unwrap_or(false) {
                    jobs.iter().map(job).collect::<Vec<_>>()
                } else {
                    jobs.par_iter().map(job).collect::<Vec<_>>()
                }
            })
            .collect()
    };
    let mut results = match pool {
        Some(p) => p.install(work),
        None => work(),
    };
    results.sort_by_key(|(pos, _)| *pos);
    results.into_iter().map(|(_, r)| r).collect()
}

/// `true` when `path` is one of the copies of `group` (or one of their links).
#[must_use]
pub fn group_has(group: &DupeGroup, path: &Path) -> bool {
    group
        .copies
        .iter()
        .any(|c| c.path == path || c.links.iter().any(|l| l == path))
}

/// Short markers for a copy's flags.
pub fn flag_text(f: CopyFlags) -> String {
    let mut out = Vec::new();
    if f.cloud {
        out.push("cloud-sync");
    }
    if f.git {
        out.push("git");
    }
    if f.app_dir {
        out.push("app");
    }
    if f.readonly {
        out.push("read-only");
    }
    out.join(" ")
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// One line per copy (and per hard-linked name).
pub fn write_csv(r: &DupeReport, out: &mut impl std::io::Write) -> std::io::Result<()> {
    writeln!(out, "group,hash,size,wasted,path,mtime,flags")?;
    for g in &r.groups {
        for c in &g.copies {
            for p in std::iter::once(&c.path).chain(&c.links) {
                writeln!(
                    out,
                    "{},{},{},{},{},{},{}",
                    g.id,
                    g.hash,
                    g.size,
                    g.wasted,
                    csv_field(&p.to_string_lossy()),
                    c.mtime,
                    flag_text(c.flags)
                )?;
            }
        }
    }
    Ok(())
}

/// Serde helpers: numbers that may not fit a JSON number, as hex text.
mod hexnum {
    pub mod u64 {
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(&format!("{v:016x}"))
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
            let text = String::deserialize(d)?;
            u64::from_str_radix(&text, 16).map_err(serde::de::Error::custom)
        }
    }

    pub mod opt_u128 {
        use serde::{Deserialize, Deserializer, Serializer};

        #[allow(clippy::ref_option)]
        pub fn serialize<S: Serializer>(v: &Option<u128>, s: S) -> Result<S::Ok, S::Error> {
            match v {
                Some(v) => s.serialize_str(&format!("{v:032x}")),
                None => s.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u128>, D::Error> {
            Option::<String>::deserialize(d)?
                .map(|t| u128::from_str_radix(&t, 16).map_err(serde::de::Error::custom))
                .transpose()
        }
    }
}

#[cfg(test)]
mod tests;
