//! Similar photos (`docs/SIMILAR_PHOTOS.md`): pictures that look the same without
//! being byte-identical. The same photo at another resolution, re-compressed by a
//! chat app, exported twice, with other metadata or slightly cropped.
//!
//! 1. **Candidates** come from [`crate::collect`], so every exclusion of the exact
//!    search holds (SafetyGate, AI conversation history, cloud placeholders never
//!    opened, links, SYSTEM files, application folders), limited to picture
//!    extensions ([`IMAGE_EXTS`]) and to files of at least [`SIMILAR_MIN_SIZE`] (and
//!    at most [`MAX_FILE`]). Several names of one file count once.
//! 2. **Fingerprint** ([`fingerprint`]): each picture is decoded within hard limits
//!    ([`decode`]), turned upright by its EXIF orientation, reduced to brightness
//!    and hashed (512-bit gradient hash plus a 64-bit coarse one). Threads run in
//!    background mode, spinning disks one file at a time. Fingerprints are cached in
//!    the history database ([`cache`]) by volume, file id, size and last write.
//! 3. **Groups** ([`HashedSet::report`]): two pictures look alike when their aspect
//!    ratios are close ([`MAX_ASPECT`]) and both hashes are within the [`Preset`].
//!    Pictures are taken best first (most pixels, then largest file); each one not
//!    yet grouped starts a group with every later look-alike of *it*, so a group
//!    never chains A to C through B.
//!
//! Byte-identical copies inside a group are marked ([`Photo::identical_to`]) and not
//! counted in [`SimilarGroup::extra`]: they belong to the exact search.
//!
//! Similar is not identical: nothing here is pre-selected. [`plan`] turns the
//! user's own picks into an ordinary hand-picked deletion (the gate's manual rules,
//! size and last write checked again right before each file) and refuses a group
//! that would lose every photo unless the user ticked "keep none" for it.

pub mod cache;
pub mod decode;
pub mod fingerprint;
pub mod plan;

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nfd_core::safety::SafetyGate;
use nfd_core::scanner::ScanTree;
use nfd_history::ImageHashRow;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cache::CacheKey;
use crate::collect::{self, Cand, Skips};
use crate::hash::{self, Ctx, Refused, Source};
use crate::keep::{KeepStrategy, keeper};
use crate::{Cancelled, CopyFlags, DupeGroup, DupeOptions, FileCopy, SkipReason};

use self::cache::ImageCache;
use self::decode::Fail;
use self::fingerprint::{Fingerprint, coarse_distance, fine_distance};

/// Extensions looked at (lower case, no dot). TIFF, HEIC and RAW files are left
/// out: no small pure-Rust decoder for them.
pub const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "jpe", "jfif", "png", "webp", "gif", "bmp"];
/// Smaller files are icons, emoji and web graphics, not photos (10 KiB).
pub const SIMILAR_MIN_SIZE: u64 = 10 << 10;
/// Larger files are not read (64 MiB): no photo is that big, a crafted file may be.
pub const MAX_FILE: u64 = 64 << 20;
/// Version of the fingerprint: cached rows of another version are decoded again.
pub const ALGO: u32 = 1;
/// Pictures whose width / height ratios differ more than this never group (a 4:3
/// photo and its 16:9 crop are different pictures).
pub const MAX_ASPECT: f64 = 1.2;
/// Pictures decoded at once on an SSD / NVMe drive by default.
const DEFAULT_THREADS: usize = 4;

/// How alike two pictures must be.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// The same picture resized, re-compressed (chat apps), re-exported or with
    /// other metadata. The default: screenshots that differ by a few lines of text
    /// can look alike, so the default stays tight.
    #[default]
    Strict,
    /// Also stronger re-compression, small edits and slight crops.
    Normal,
    /// Also visible crops and edits; bursts of the same scene may join.
    Loose,
}

impl Preset {
    /// Fine-hash bits (of 512) that may differ.
    #[must_use]
    pub const fn max_distance(self) -> u32 {
        match self {
            Self::Strict => 40,
            Self::Normal => 64,
            Self::Loose => 96,
        }
    }

    /// Coarse-hash bits (of 64) that may differ.
    #[must_use]
    pub const fn max_coarse(self) -> u32 {
        match self {
            Self::Strict => 10,
            Self::Normal => 14,
            Self::Loose => 20,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Normal => "normal",
            Self::Loose => "loose",
        }
    }
}

impl FromStr for Preset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "strict" => Ok(Self::Strict),
            "normal" => Ok(Self::Normal),
            "loose" => Ok(Self::Loose),
            _ => Err(format!("unknown similarity `{s}` (strict, normal, loose)")),
        }
    }
}

/// What to look at.
#[derive(Debug, Clone)]
pub struct SimilarOptions {
    /// Smaller pictures are left out.
    pub min_size: u64,
    /// Also look inside application folders (only shown, never suggested).
    pub include_app_dirs: bool,
    /// Pictures decoded at once per SSD (spinning disks: one).
    pub threads: Option<NonZeroUsize>,
    /// Application folders by location ([`collect::default_app_dirs`]).
    pub app_dir_paths: Vec<PathBuf>,
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Default for SimilarOptions {
    fn default() -> Self {
        Self {
            min_size: SIMILAR_MIN_SIZE,
            include_app_dirs: false,
            threads: None,
            app_dir_paths: Vec::new(),
            cancel: None,
        }
    }
}

/// Step of the search, for progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SimilarPhase {
    Collect,
    /// Decoding and fingerprinting.
    Hash,
    /// Comparing fingerprints.
    Compare,
}

/// Progress of one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarProgress {
    pub phase: SimilarPhase,
    pub done: u64,
    pub total: u64,
}

/// Why a picture was not compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SimilarSkip {
    Small,
    Cloud,
    Reparse,
    System,
    Protected,
    AppDir,
    /// Not a picture this build reads, damaged or cut short.
    Unreadable,
    /// Changed while it was read.
    Changed,
    /// Over the file size or pixel limits.
    TooLarge,
    /// No detail to compare (a plain colour, a blank page) or smaller than 32 pixels.
    Plain,
}

impl SimilarSkip {
    /// The exact search's reason, when it applies to pictures.
    const fn from_skip(r: SkipReason) -> Option<Self> {
        match r {
            SkipReason::Small => Some(Self::Small),
            SkipReason::Cloud => Some(Self::Cloud),
            SkipReason::Reparse => Some(Self::Reparse),
            SkipReason::System => Some(Self::System),
            SkipReason::Protected => Some(Self::Protected),
            SkipReason::AppDir => Some(Self::AppDir),
            SkipReason::Unreadable => Some(Self::Unreadable),
            SkipReason::Changed => Some(Self::Changed),
            // other files and empty files are not pictures at all
            SkipReason::Empty | SkipReason::Extension | SkipReason::Clone => None,
        }
    }

    /// Code stored in the cache (0 = hashed). Only lasting reasons are stored.
    const fn status(self) -> Option<u8> {
        match self {
            Self::Unreadable => Some(1),
            Self::TooLarge => Some(2),
            Self::Plain => Some(3),
            _ => None,
        }
    }

    const fn from_status(s: u8) -> Self {
        match s {
            2 => Self::TooLarge,
            3 => Self::Plain,
            _ => Self::Unreadable,
        }
    }
}

/// One picture left out, as an example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarSkipped {
    pub path: PathBuf,
    pub reason: SimilarSkip,
}

/// A picture in a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub path: PathBuf,
    /// Other names of the same file (hard links): removed with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<PathBuf>,
    pub size: u64,
    /// Last write, Unix seconds.
    pub mtime: i64,
    /// Upright size in pixels.
    pub width: u32,
    pub height: u32,
    #[serde(with = "crate::hexnum::u64")]
    pub volume: u64,
    #[serde(with = "crate::hexnum::opt_u128")]
    pub file_id: Option<u128>,
    pub flags: CopyFlags,
    /// Fine-hash bits that differ from the group's first (best) photo.
    pub distance: u32,
    /// Byte for byte the same file as this earlier photo of the group (the exact
    /// search lists it too).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identical_to: Option<PathBuf>,
}

/// Pictures that look alike. The first photo is the suggested best one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarGroup {
    /// 1-based, in report order.
    pub id: u32,
    pub photos: Vec<Photo>,
    /// Bytes of every photo but the first, byte-identical copies left out.
    pub extra: u64,
}

/// Result of a search for one preset.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarReport {
    pub roots: Vec<PathBuf>,
    pub min_size: u64,
    pub preset: Preset,
    /// Largest `extra` first.
    pub groups: Vec<SimilarGroup>,
    pub extra_bytes: u64,
    /// Files looked at in the scans.
    pub scanned_files: u64,
    /// Pictures fingerprinted (decoded now or from the cache).
    pub images: u64,
    /// Pictures decoded in this run.
    pub decoded: u64,
    pub cache_hits: u64,
    pub skipped: BTreeMap<SimilarSkip, u64>,
    pub skipped_samples: Vec<SimilarSkipped>,
    pub elapsed_ms: u64,
}

/// A fingerprinted picture.
#[derive(Debug, Clone)]
pub struct HashedPhoto {
    pub path: PathBuf,
    pub links: Vec<PathBuf>,
    pub size: u64,
    pub mtime: i64,
    pub volume: u64,
    pub file_id: Option<u128>,
    pub flags: CopyFlags,
    pub width: u32,
    pub height: u32,
    pub fingerprint: Fingerprint,
    /// BLAKE3 of the contents, only for pictures that may be byte-identical to
    /// another one (same size and fingerprint).
    pub content: Option<[u8; 32]>,
}

impl HashedPhoto {
    fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// `ln(width / height)`.
    fn aspect(&self) -> f64 {
        (f64::from(self.width.max(1)) / f64::from(self.height.max(1))).ln()
    }
}

/// Every picture of a search with its fingerprint: grouped again for another
/// preset without reading anything.
#[derive(Debug, Clone, Default)]
pub struct HashedSet {
    pub roots: Vec<PathBuf>,
    pub min_size: u64,
    pub scanned_files: u64,
    pub decoded: u64,
    pub cache_hits: u64,
    pub skipped: BTreeMap<SimilarSkip, u64>,
    pub skipped_samples: Vec<SimilarSkipped>,
    pub elapsed_ms: u64,
    pub photos: Vec<HashedPhoto>,
}

/// Examples kept per run.
const SAMPLES: usize = 200;

#[derive(Debug, Default)]
struct SkipLog {
    counts: BTreeMap<SimilarSkip, u64>,
    samples: Vec<SimilarSkipped>,
}

impl SkipLog {
    fn add(&mut self, reason: SimilarSkip, path: impl FnOnce() -> PathBuf) {
        *self.counts.entry(reason).or_default() += 1;
        if self.samples.len() < SAMPLES && reason != SimilarSkip::Small {
            self.samples.push(SimilarSkipped {
                path: path(),
                reason,
            });
        }
    }
}

fn cache_key(c: &Cand) -> Option<CacheKey> {
    Some(CacheKey {
        volume: c.volume,
        file_id: c.file_id?,
        size: c.size,
        mtime: c.mtime,
    })
}

/// One picture to fingerprint: a candidate and its other names.
#[derive(Debug)]
struct Unit {
    cand: usize,
    links: Vec<usize>,
    /// Fingerprint and upright size, or why not.
    result: Option<Result<(Fingerprint, u32, u32), SimilarSkip>>,
    fresh: bool,
}

/// Why a picture got no fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Cancelled,
    Skip(SimilarSkip),
}

impl From<Refused> for Stop {
    fn from(r: Refused) -> Self {
        match r {
            Refused::Cancelled => Self::Cancelled,
            Refused::Skip(reason) => {
                Self::Skip(SimilarSkip::from_skip(reason).unwrap_or(SimilarSkip::Unreadable))
            }
        }
    }
}

impl From<std::io::Error> for Stop {
    fn from(_: std::io::Error) -> Self {
        Self::Skip(SimilarSkip::Unreadable)
    }
}

/// Read and fingerprint one picture. Never panics: a decoder panic on a crafted
/// file becomes [`SimilarSkip::Unreadable`].
fn hash_one(source: &dyn Source, c: &Cand, ctx: &Ctx<'_>) -> Result<(Fingerprint, u32, u32), Stop> {
    if ctx.cancelled() {
        return Err(Stop::Cancelled);
    }
    // never opens a cloud placeholder or a link, refuses a file that changed
    hash::check(source, &c.path, c.size, c.mtime)?;
    let mut data = Vec::with_capacity(usize::try_from(c.size).unwrap_or(0));
    source
        .open(&c.path)?
        .take(MAX_FILE + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 != c.size {
        return Err(Stop::Skip(SimilarSkip::Changed));
    }
    ctx.bytes.fetch_add(c.size, Ordering::Relaxed);
    let decoded = catch_unwind(AssertUnwindSafe(|| decode::decode(&data)));
    drop(data);
    hash::check(source, &c.path, c.size, c.mtime)?;
    let luma = match decoded {
        Ok(Ok(l)) => l,
        Ok(Err(Fail::TooLarge)) => return Err(Stop::Skip(SimilarSkip::TooLarge)),
        Ok(Err(Fail::Unreadable)) | Err(_) => return Err(Stop::Skip(SimilarSkip::Unreadable)),
    };
    let (w, h) = (
        u32::try_from(luma.width).unwrap_or(u32::MAX),
        u32::try_from(luma.height).unwrap_or(u32::MAX),
    );
    // tiny or without detail: nothing to compare
    fingerprint::fingerprint(&luma.px, luma.width, luma.height)
        .filter(|(_, sd)| *sd >= fingerprint::PLAIN_SD)
        .map(|(fp, _)| (fp, w, h))
        .ok_or(Stop::Skip(SimilarSkip::Plain))
}

/// Fingerprint every picture of `trees`.
///
/// `progress` is called from worker threads at most every 100 ms (and at the end
/// of each step).
///
/// # Errors
/// [`Cancelled`] when `opts.cancel` was raised.
pub fn find_hashed(
    trees: &[&ScanTree],
    gate: &SafetyGate,
    opts: &SimilarOptions,
    cache: Option<&mut dyn ImageCache>,
    source: &dyn Source,
    progress: &(dyn Fn(&SimilarProgress) + Sync),
) -> Result<HashedSet, Cancelled> {
    let started = Instant::now();
    let ctx = Ctx {
        cancel: opts.cancel.as_deref(),
        bytes: AtomicU64::new(0),
    };
    progress(&SimilarProgress {
        phase: SimilarPhase::Collect,
        done: 0,
        total: 0,
    });
    let collect_opts = DupeOptions {
        min_size: 1,
        include_app_dirs: opts.include_app_dirs,
        extensions: IMAGE_EXTS.iter().map(|e| (*e).to_owned()).collect(),
        app_dir_paths: opts.app_dir_paths.clone(),
        ..DupeOptions::default()
    };
    let mut skips = Skips::default();
    let (all, scanned) = collect::collect(trees, gate, &collect_opts, source, &mut skips);
    if ctx.cancelled() {
        return Err(Cancelled);
    }
    let mut log = SkipLog::default();
    for (reason, n) in &skips.counts {
        if let Some(r) = SimilarSkip::from_skip(*reason) {
            *log.counts.entry(r).or_default() += n;
        }
    }
    for s in skips.samples {
        if let Some(r) = SimilarSkip::from_skip(s.reason) {
            if log.samples.len() < SAMPLES {
                log.samples.push(SimilarSkipped {
                    path: s.path,
                    reason: r,
                });
            }
        }
    }
    let min = opts.min_size.max(1);
    let mut cands: Vec<Cand> = Vec::with_capacity(all.len());
    for c in all {
        if c.size < min {
            log.add(SimilarSkip::Small, || c.path.clone());
        } else if c.size > MAX_FILE {
            log.add(SimilarSkip::TooLarge, || c.path.clone());
        } else {
            cands.push(c);
        }
    }

    // several names of one file: one picture
    let mut units: Vec<Unit> = Vec::new();
    let mut seen: HashMap<(u64, u128), usize> = HashMap::new();
    for (i, c) in cands.iter().enumerate() {
        if let Some(id) = c.file_id {
            if let Some(&u) = seen.get(&(c.volume, id)) {
                units[u].links.push(i);
                continue;
            }
            seen.insert((c.volume, id), units.len());
        }
        units.push(Unit {
            cand: i,
            links: Vec::new(),
            result: None,
            fresh: false,
        });
    }

    // cached fingerprints
    let mut cache_hits = 0u64;
    if let Some(cache) = cache.as_deref() {
        let wanted: Vec<(usize, CacheKey)> = units
            .iter()
            .enumerate()
            .filter_map(|(u, unit)| cache_key(&cands[unit.cand]).map(|k| (u, k)))
            .collect();
        let keys: Vec<CacheKey> = wanted.iter().map(|(_, k)| *k).collect();
        for ((u, _), hit) in wanted.iter().zip(cache.get(&keys, ALGO)) {
            let Some(row) = hit else { continue };
            let result = if row.status == 0 {
                match row.hash.as_deref().and_then(Fingerprint::from_bytes) {
                    Some(fp) => Ok((fp, row.width, row.height)),
                    None => continue,
                }
            } else {
                Err(SimilarSkip::from_status(row.status))
            };
            units[*u].result = Some(result);
            cache_hits += 1;
        }
    }

    // decode the rest
    let todo: Vec<usize> = (0..units.len())
        .filter(|&u| units[u].result.is_none())
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads.map_or(DEFAULT_THREADS, NonZeroUsize::get))
        .thread_name(|i| format!("nfd-similar-{i}"))
        .start_handler(|_| nfd_platform::io::background_priority())
        .build()
        .ok();
    let mut hdd: HashMap<u64, bool> = HashMap::new();
    for &u in &todo {
        let c = &cands[units[u].cand];
        hdd.entry(c.volume)
            .or_insert_with(|| source.seek_penalty(&c.path) == Some(true));
    }
    let results = run(pool.as_ref(), &todo, &units, &cands, &hdd, progress, |c| {
        hash_one(source, c, &ctx)
    });
    let mut decoded = 0u64;
    for (&u, r) in todo.iter().zip(results) {
        match r {
            Ok(v) => {
                units[u].result = Some(Ok(v));
                units[u].fresh = true;
                decoded += 1;
            }
            Err(Stop::Cancelled) => return Err(Cancelled),
            Err(Stop::Skip(r)) => {
                units[u].result = Some(Err(r));
                // only lasting answers are remembered
                units[u].fresh = r.status().is_some();
                decoded += u64::from(r.status().is_some());
            }
        }
    }

    // remember what was decoded
    if let Some(cache) = cache {
        let rows: Vec<(CacheKey, ImageHashRow)> = units
            .iter()
            .filter(|u| u.fresh)
            .filter_map(|u| {
                let k = cache_key(&cands[u.cand])?;
                let row = match u.result? {
                    Ok((fp, width, height)) => ImageHashRow {
                        algo: ALGO,
                        status: 0,
                        width,
                        height,
                        hash: Some(fp.to_bytes()),
                    },
                    Err(r) => ImageHashRow {
                        algo: ALGO,
                        status: r.status()?,
                        width: 0,
                        height: 0,
                        hash: None,
                    },
                };
                Some((k, row))
            })
            .collect();
        cache.put(&rows);
    }

    let mut photos: Vec<HashedPhoto> = Vec::new();
    for u in &units {
        let c = &cands[u.cand];
        match u.result {
            Some(Ok((fp, width, height))) => {
                let mut links: Vec<PathBuf> =
                    u.links.iter().map(|&l| cands[l].path.clone()).collect();
                links.sort();
                photos.push(HashedPhoto {
                    path: c.path.clone(),
                    links,
                    size: c.size,
                    mtime: c.mtime,
                    volume: c.volume,
                    file_id: c.file_id,
                    flags: c.flags,
                    width,
                    height,
                    fingerprint: fp,
                    content: None,
                });
            }
            Some(Err(r)) => log.add(r, || c.path.clone()),
            None => {}
        }
    }

    // pictures that may be byte-identical: same size, fingerprint and pixel size
    let mut same: HashMap<(u64, Fingerprint, u32, u32), Vec<usize>> = HashMap::new();
    for (i, p) in photos.iter().enumerate() {
        same.entry((p.size, p.fingerprint, p.width, p.height))
            .or_default()
            .push(i);
    }
    for idx in same.into_values().filter(|v| v.len() > 1) {
        for i in idx {
            let p = &photos[i];
            match hash::full(source, &p.path, p.size, p.mtime, &ctx) {
                Ok(d) => photos[i].content = Some(d),
                Err(Refused::Cancelled) => return Err(Cancelled),
                Err(Refused::Skip(_)) => {}
            }
        }
    }

    Ok(HashedSet {
        roots: trees.iter().map(|t| t.root.clone()).collect(),
        min_size: opts.min_size,
        scanned_files: scanned,
        decoded,
        cache_hits,
        skipped: log.counts,
        skipped_samples: log.samples,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        photos,
    })
}

/// [`find_hashed`] and [`HashedSet::report`] in one go.
///
/// # Errors
/// [`Cancelled`] when `opts.cancel` was raised.
pub fn find(
    trees: &[&ScanTree],
    gate: &SafetyGate,
    opts: &SimilarOptions,
    preset: Preset,
    cache: Option<&mut dyn ImageCache>,
    source: &dyn Source,
    progress: &(dyn Fn(&SimilarProgress) + Sync),
) -> Result<(SimilarReport, HashedSet), Cancelled> {
    let set = find_hashed(trees, gate, opts, cache, source, progress)?;
    let report = set.report(preset, opts.cancel.as_deref(), progress)?;
    Ok((report, set))
}

/// Run `f` over the units `todo` (result order = `todo` order): spinning disks one
/// file at a time in path order, other volumes in parallel.
fn run<R: Send>(
    pool: Option<&rayon::ThreadPool>,
    todo: &[usize],
    units: &[Unit],
    cands: &[Cand],
    hdd: &HashMap<u64, bool>,
    progress: &(dyn Fn(&SimilarProgress) + Sync),
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
            progress(&SimilarProgress {
                phase: SimilarPhase::Hash,
                done: d,
                total,
            });
        }
    };
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

/// `true` when `a` and `b` look alike under `preset`.
#[must_use]
pub fn alike(a: &HashedPhoto, b: &HashedPhoto, preset: Preset) -> bool {
    (a.aspect() - b.aspect()).abs() <= MAX_ASPECT.ln()
        && coarse_distance(&a.fingerprint, &b.fingerprint) <= preset.max_coarse()
        && fine_distance(&a.fingerprint, &b.fingerprint) <= preset.max_distance()
}

impl HashedSet {
    /// Group the pictures for `preset`. Every pair is compared once (two `popcount`
    /// for nearly every pair, the coarse one first): about 5 × 10⁹ cheap tests for
    /// 100,000 pictures, spread over the threads, a few seconds.
    ///
    /// # Errors
    /// [`Cancelled`] when `cancel` was raised.
    pub fn report(
        &self,
        preset: Preset,
        cancel: Option<&AtomicBool>,
        progress: &(dyn Fn(&SimilarProgress) + Sync),
    ) -> Result<SimilarReport, Cancelled> {
        let started = Instant::now();
        let p = &self.photos;
        // best first: most pixels, then largest file, then path
        let mut order: Vec<usize> = (0..p.len()).collect();
        order.sort_by(|&a, &b| {
            p[b].pixels()
                .cmp(&p[a].pixels())
                .then(p[b].size.cmp(&p[a].size))
                .then_with(|| p[a].path.cmp(&p[b].path))
        });
        let n = order.len();
        // the same test as `alike`, on arrays in comparison order
        let coarse: Vec<u64> = order.iter().map(|&k| p[k].fingerprint.coarse).collect();
        let aspect: Vec<f64> = order.iter().map(|&k| p[k].aspect()).collect();
        let fine: Vec<&Fingerprint> = order.iter().map(|&k| &p[k].fingerprint).collect();
        let (max_coarse, max_fine, max_aspect) =
            (preset.max_coarse(), preset.max_distance(), MAX_ASPECT.ln());
        let total = n as u64;
        let done = AtomicU64::new(0);
        let last = Mutex::new(Instant::now());
        let stop = AtomicBool::new(false);
        let compare = || -> Vec<Vec<usize>> {
            (0..n)
                .into_par_iter()
                .map(|i| {
                    if stop.load(Ordering::Relaxed)
                        || cancel.is_some_and(|c| c.load(Ordering::Relaxed))
                    {
                        stop.store(true, Ordering::Relaxed);
                        return Vec::new();
                    }
                    let near: Vec<usize> = (i + 1..n)
                        .filter(|&j| {
                            (coarse[i] ^ coarse[j]).count_ones() <= max_coarse
                                && (aspect[i] - aspect[j]).abs() <= max_aspect
                                && fine_distance(fine[i], fine[j]) <= max_fine
                        })
                        .collect();
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
                        progress(&SimilarProgress {
                            phase: SimilarPhase::Compare,
                            done: d,
                            total,
                        });
                    }
                    near
                })
                .collect()
        };
        // background threads, like the decoding
        let neighbours = match rayon::ThreadPoolBuilder::new()
            .num_threads(DEFAULT_THREADS)
            .thread_name(|i| format!("nfd-similar-cmp-{i}"))
            .start_handler(|_| nfd_platform::io::background_priority())
            .build()
        {
            Ok(pool) => pool.install(compare),
            Err(_) => compare(),
        };
        if stop.load(Ordering::Relaxed) {
            return Err(Cancelled);
        }

        let mut taken = vec![false; n];
        let mut groups: Vec<SimilarGroup> = Vec::new();
        for i in 0..n {
            if taken[i] {
                continue;
            }
            taken[i] = true;
            let mut members = vec![i];
            for &j in &neighbours[i] {
                if !taken[j] {
                    taken[j] = true;
                    members.push(j);
                }
            }
            if members.len() > 1 {
                groups.push(self.group(members.iter().map(|&m| order[m]).collect()));
            }
        }
        groups.sort_by(|a, b| {
            b.extra
                .cmp(&a.extra)
                .then_with(|| a.photos[0].path.cmp(&b.photos[0].path))
        });
        for (k, g) in groups.iter_mut().enumerate() {
            g.id = u32::try_from(k + 1).unwrap_or(u32::MAX);
        }
        Ok(SimilarReport {
            roots: self.roots.clone(),
            min_size: self.min_size,
            preset,
            extra_bytes: groups.iter().map(|g| g.extra).sum(),
            groups,
            scanned_files: self.scanned_files,
            images: self.photos.len() as u64,
            decoded: self.decoded,
            cache_hits: self.cache_hits,
            skipped: self.skipped.clone(),
            skipped_samples: self.skipped_samples.clone(),
            elapsed_ms: self.elapsed_ms
                + u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }

    /// One group of the photos `members` (indices into `photos`, best first).
    fn group(&self, mut members: Vec<usize>) -> SimilarGroup {
        let p = &self.photos;
        // among photos as good as the first, the keep rules of the exact search
        // decide (risky locations last, Downloads / Desktop / Temp after the rest,
        // then the oldest)
        let lead = members[0];
        let tied: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&m| p[m].pixels() == p[lead].pixels() && p[m].size == p[lead].size)
            .collect();
        if tied.len() > 1 {
            let as_dupe = DupeGroup {
                id: 0,
                hash: String::new(),
                size: p[lead].size,
                wasted: 0,
                copies: tied
                    .iter()
                    .map(|&m| FileCopy {
                        path: p[m].path.clone(),
                        mtime: p[m].mtime,
                        volume: p[m].volume,
                        file_id: p[m].file_id,
                        links: Vec::new(),
                        flags: p[m].flags,
                        link: None,
                    })
                    .collect(),
            };
            if let Some(k) = keeper(&as_dupe, &KeepStrategy::Auto) {
                let best = tied[k];
                members.retain(|&m| m != best);
                members.insert(0, best);
            }
        }
        let first = &p[members[0]].fingerprint;
        let mut photos: Vec<Photo> = Vec::with_capacity(members.len());
        let mut extra = 0;
        for (n, &m) in members.iter().enumerate() {
            let h = &p[m];
            let identical_to = h.content.and_then(|c| {
                members[..n]
                    .iter()
                    .find(|&&o| p[o].content == Some(c))
                    .map(|&o| p[o].path.clone())
            });
            if n > 0 && identical_to.is_none() {
                extra += h.size;
            }
            photos.push(Photo {
                path: h.path.clone(),
                links: h.links.clone(),
                size: h.size,
                mtime: h.mtime,
                width: h.width,
                height: h.height,
                volume: h.volume,
                file_id: h.file_id,
                flags: h.flags,
                distance: fine_distance(first, &h.fingerprint),
                identical_to,
            });
        }
        SimilarGroup {
            id: 0,
            photos,
            extra,
        }
    }

    /// Forget pictures that left the disk (`gone(path)`), so a later regrouping
    /// does not bring them back.
    pub fn forget(&mut self, gone: impl Fn(&Path) -> bool) {
        self.photos
            .retain(|p| !gone(&p.path) && !p.links.iter().any(|l| gone(l)));
    }
}

/// Drop removed photos from `report` (groups left with one photo go too).
pub fn forget(report: &mut SimilarReport, gone: impl Fn(&Path) -> bool) {
    for g in &mut report.groups {
        g.photos.retain(|p| !gone(&p.path));
        // the first photo stays the reference of `extra` even when another went
        g.extra = g
            .photos
            .iter()
            .skip(1)
            .filter(|p| p.identical_to.is_none())
            .map(|p| p.size)
            .sum();
    }
    report.groups.retain(|g| g.photos.len() > 1);
    report.extra_bytes = report.groups.iter().map(|g| g.extra).sum();
}

/// `true` when `path` is a photo (or one of its links) of `group`.
#[must_use]
pub fn group_has(group: &SimilarGroup, path: &Path) -> bool {
    group
        .photos
        .iter()
        .any(|p| p.path == path || p.links.iter().any(|l| l == path))
}

#[cfg(test)]
mod tests;
