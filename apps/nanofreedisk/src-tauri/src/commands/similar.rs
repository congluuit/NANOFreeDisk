//! "Similar photos" in the Duplicates view (`docs/SIMILAR_PHOTOS.md`): pictures
//! that look alike without being byte-identical, found by perceptual hashes in the
//! user's personal folders, the Analyze scan or folders picked here.
//!
//! Nothing is pre-selected. Removal is an ordinary hand-picked deletion through
//! `Planner → SafetyGate → Executor → UndoJournal` ([`super::clean`]): the photos
//! the user ticked, checked by the gate's manual rules, with size and last write
//! compared right before each move and a group never emptied unless the user ticked
//! "keep none" for it. Fingerprints are cached in the history database unless the
//! caller opts out (`useCache: false`, as the GUI smoke test does). Only photos of
//! the last search can be revealed, previewed or removed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use nfd_core::safety::SafetyGate;
use nfd_core::scanner::{self, ScanEvent, ScanOptions, ScanTree};
use nfd_dupes::hash::{LiveSource, Source};
use nfd_dupes::plan::Action;
use nfd_dupes::similar::cache::ImageCache;
use nfd_dupes::similar::plan::{SimilarSelection, SimilarVerifier};
use nfd_dupes::similar::{
    self, HashedSet, Preset, SimilarOptions, SimilarPhase, SimilarProgress, SimilarReport,
};
use nfd_platform::Platform;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use super::clean::{self, OperationDto};
use super::{AppState, CmdResult, err};

/// Groups sent to the UI at most (largest first); the rest stay counted.
const MAX_GROUPS: usize = 2_000;
/// In the side-by-side view, pictures up to this size are shown as they are.
const RAW_MAX: u64 = 8 << 20;
/// Longer side of a grid preview (tiles are about 160 pixels, twice for HiDPI).
const GRID_SIDE: u32 = 320;
/// Longer side of the side-by-side preview of a larger picture.
const LARGE_SIDE: u32 = 1600;

/// The last search (fingerprints and groups) and the running one, if any.
#[derive(Debug, Default)]
pub struct SimilarState {
    hashed: Mutex<Option<HashedSet>>,
    report: Mutex<Option<SimilarReport>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl SimilarState {
    /// `true` when `path` is a photo (or a link of one) of the last search.
    fn knows(&self, path: &Path) -> CmdResult<bool> {
        let guard = self.report.lock().map_err(|e| err("internal", e))?;
        Ok(guard
            .as_ref()
            .is_some_and(|r| r.groups.iter().any(|g| similar::group_has(g, path))))
    }
}

/// What the UI asks for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindOptions {
    /// Smaller pictures are left out (default 10 KiB).
    #[serde(default)]
    min_size: Option<u64>,
    #[serde(default)]
    preset: Preset,
    #[serde(default)]
    include_app_dirs: bool,
    /// Read and write the fingerprint cache in the history database.
    #[serde(default = "yes")]
    use_cache: bool,
    /// Search the user's personal folders instead of `paths`.
    #[serde(default)]
    personal: bool,
}

const fn yes() -> bool {
    true
}

/// Progress event `similar-progress`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressDto {
    /// `scan`, `collect`, `hash` or `compare`.
    phase: &'static str,
    done: u64,
    total: u64,
    path: String,
}

/// The search result as the UI gets it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarView {
    #[serde(flatten)]
    report: SimilarReport,
    /// Groups found (the list holds at most [`MAX_GROUPS`]).
    total_groups: usize,
}

fn view(report: &SimilarReport) -> SimilarView {
    let mut shown = report.clone();
    shown.groups.truncate(MAX_GROUPS);
    SimilarView {
        total_groups: report.groups.len(),
        report: shown,
    }
}

const fn phase_name(p: SimilarPhase) -> &'static str {
    match p {
        SimilarPhase::Collect => "collect",
        SimilarPhase::Hash => "hash",
        SimilarPhase::Compare => "compare",
    }
}

fn emitter(app: &AppHandle) -> impl Fn(&SimilarProgress) + Sync + '_ {
    move |p: &SimilarProgress| {
        let _ = app.emit(
            "similar-progress",
            ProgressDto {
                phase: phase_name(p.phase),
                done: p.done,
                total: p.total,
                path: String::new(),
            },
        );
    }
}

/// Scan `path` for the search (not recorded, Analyze untouched), streaming progress.
fn scan(app: &AppHandle, path: &Path, cancel: &Arc<AtomicBool>) -> CmdResult<ScanTree> {
    let options = ScanOptions {
        cancel: Some(cancel.clone()),
        ..ScanOptions::default()
    };
    let (tx, rx) = mpsc::channel();
    let root = path.to_path_buf();
    let worker = std::thread::spawn(move || scanner::scan(&root, &options, Some(&tx)));
    let shown = path.to_string_lossy().into_owned();
    for event in rx {
        match event {
            ScanEvent::Progress(s) => {
                let _ = app.emit(
                    "similar-progress",
                    ProgressDto {
                        phase: "scan",
                        done: s.files,
                        total: 0,
                        path: shown.clone(),
                    },
                );
            }
            ScanEvent::Done { .. } => break,
        }
    }
    match worker.join() {
        Ok(Ok(tree)) => Ok(tree),
        Ok(Err(scanner::ScanError::Cancelled)) => Err(err("cancelled", "search cancelled")),
        Ok(Err(e)) => Err(err("scan", e)),
        Err(_) => Err(err("internal", "scan thread panicked")),
    }
}

/// Find look-alike photos in `paths` (folders or drives), in the user's personal
/// folders with `options.personal`, or in the current Analyze scan when `paths` is
/// empty. Progress streams as `similar-progress`; [`similar_cancel`] stops it.
#[tauri::command]
pub async fn similar_find(
    app: AppHandle,
    state: State<'_, AppState>,
    similar: State<'_, SimilarState>,
    paths: Vec<String>,
    options: FindOptions,
) -> CmdResult<SimilarView> {
    let paths: Vec<PathBuf> = if options.personal {
        let found = nfd_dupes::personal::personal_folders();
        if found.is_empty() {
            return Err(err("not-found", "no personal folders"));
        }
        found
    } else {
        nfd_dupes::personal::outermost(paths.into_iter().map(PathBuf::from).collect())
    };
    let current = if paths.is_empty() {
        let guard = state.session.read().map_err(|e| err("internal", e))?;
        Some(
            guard
                .as_ref()
                .map(|s| s.tree.clone())
                .ok_or_else(super::no_session)?,
        )
    } else {
        None
    };
    for p in &paths {
        if !p.is_dir() {
            return Err(err("not-found", p.to_string_lossy()));
        }
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *similar.cancel.lock().map_err(|e| err("internal", e))? = Some(cancel.clone());
    let db = if options.use_cache {
        super::history_path(&app).ok()
    } else {
        None
    };
    let app2 = app.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || -> CmdResult<(SimilarReport, HashedSet)> {
            let trees: Vec<ScanTree> = match current {
                Some(t) => vec![t],
                None => paths
                    .iter()
                    .map(|p| scan(&app2, p, &cancel))
                    .collect::<CmdResult<_>>()?,
            };
            let resolve = nfd_platform::Native.resolver();
            let gate = SafetyGate::new(&resolve);
            let opts = SimilarOptions {
                min_size: options.min_size.unwrap_or(similar::SIMILAR_MIN_SIZE).max(1),
                include_app_dirs: options.include_app_dirs,
                threads: None,
                app_dir_paths: nfd_dupes::collect::default_app_dirs(&resolve),
                cancel: Some(cancel.clone()),
            };
            // a connection of its own: the app's shared one stays free meanwhile
            let mut history = db.and_then(|p| nfd_history::History::open(&p).ok());
            let refs: Vec<&ScanTree> = trees.iter().collect();
            similar::find(
                &refs,
                &gate,
                &opts,
                options.preset,
                history.as_mut().map(|h| h as &mut dyn ImageCache),
                &LiveSource,
                &emitter(&app2),
            )
            .map_err(|_| err("cancelled", "search cancelled"))
        })
        .await
        .map_err(|e| err("internal", e))?;
    *similar.cancel.lock().map_err(|e| err("internal", e))? = None;
    let (report, hashed) = result?;
    let out = view(&report);
    *similar.report.lock().map_err(|e| err("internal", e))? = Some(report);
    *similar.hashed.lock().map_err(|e| err("internal", e))? = Some(hashed);
    Ok(out)
}

/// Group the last search again for another similarity preset (nothing is read).
#[tauri::command]
pub async fn similar_regroup(
    app: AppHandle,
    similar: State<'_, SimilarState>,
    preset: Preset,
) -> CmdResult<SimilarView> {
    let set = similar
        .hashed
        .lock()
        .map_err(|e| err("internal", e))?
        .take()
        .ok_or_else(|| err("no-similar", "no similar-photos search yet"))?;
    let cancel = Arc::new(AtomicBool::new(false));
    *similar.cancel.lock().map_err(|e| err("internal", e))? = Some(cancel.clone());
    let app2 = app.clone();
    let (set, result) = tauri::async_runtime::spawn_blocking(move || {
        let r = set.report(preset, Some(cancel.as_ref()), &emitter(&app2));
        (set, r)
    })
    .await
    .map_err(|e| err("internal", e))?;
    *similar.cancel.lock().map_err(|e| err("internal", e))? = None;
    *similar.hashed.lock().map_err(|e| err("internal", e))? = Some(set);
    let report = result.map_err(|_| err("cancelled", "search cancelled"))?;
    let out = view(&report);
    *similar.report.lock().map_err(|e| err("internal", e))? = Some(report);
    Ok(out)
}

/// Stop the running search or removal.
#[tauri::command]
pub fn similar_cancel(similar: State<'_, SimilarState>) -> CmdResult<()> {
    if let Some(c) = similar
        .cancel
        .lock()
        .map_err(|e| err("internal", e))?
        .as_ref()
    {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// The last search again (after a removal it no longer lists removed photos).
#[tauri::command]
pub fn similar_report(similar: State<'_, SimilarState>) -> CmdResult<Option<SimilarView>> {
    let guard = similar.report.lock().map_err(|e| err("internal", e))?;
    Ok(guard.as_ref().map(view))
}

/// Remove the photos the user ticked (Recycle Bin or quarantine). The planner
/// refuses a group that would lose every photo unless its `keepNone` is set; each
/// photo is checked again right before the move. Progress streams as
/// `clean-progress`.
#[tauri::command]
pub async fn similar_run(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    similar: State<'_, SimilarState>,
    selections: Vec<SimilarSelection>,
    action: Action,
) -> CmdResult<OperationDto> {
    let (plan, groups) = {
        let guard = similar.report.lock().map_err(|e| err("internal", e))?;
        let report = guard
            .as_ref()
            .ok_or_else(|| err("no-similar", "no similar-photos search yet"))?;
        let gate = SafetyGate::new(&nfd_platform::Native.resolver());
        let plan = similar::plan::plan(&report.groups, &selections, action, &gate)
            .map_err(|e| err("similar-plan", e))?;
        let ids: HashSet<u32> = selections.iter().map(|s| s.group).collect();
        let groups: Vec<_> = report
            .groups
            .iter()
            .filter(|g| ids.contains(&g.id))
            .cloned()
            .collect();
        (plan, groups)
    };
    if plan.items.is_empty() {
        return Err(err("nothing-to-clean", "nothing selected"));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *similar.cancel.lock().map_err(|e| err("internal", e))? = Some(cancel.clone());
    let mut verifier = SimilarVerifier::new(&LiveSource, &groups, &selections);
    let (dto, op) = clean::execute_plan_returning(
        &app,
        &window,
        &state,
        plan,
        "similar",
        cancel,
        Box::new(move |item| verifier.check(item)),
    )
    .await?;
    *similar.cancel.lock().map_err(|e| err("internal", e))? = None;
    // forget what left the disk
    let gone: HashSet<PathBuf> = op
        .items
        .iter()
        .filter(|i| i.status.removed_something())
        .map(|i| i.path.clone())
        .collect();
    if let Some(report) = similar
        .report
        .lock()
        .map_err(|e| err("internal", e))?
        .as_mut()
    {
        similar::forget(report, |p| gone.contains(p));
    }
    if let Some(set) = similar
        .hashed
        .lock()
        .map_err(|e| err("internal", e))?
        .as_mut()
    {
        set.forget(|p| gone.contains(p));
    }
    Ok(dto)
}

/// Show a photo of the last search in Explorer / Finder.
#[tauri::command]
pub fn similar_reveal(
    app: AppHandle,
    similar: State<'_, SimilarState>,
    path: String,
) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = PathBuf::from(&path);
    if !similar.knows(&p)? {
        return Err(err("not-found", path));
    }
    app.opener()
        .reveal_item_in_dir(&p)
        .map_err(|e| err("os", e))
}

/// MIME type of a picture by its first bytes (the extension may lie).
fn sniff(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(b"GIF8") {
        Some("image/gif")
    } else if data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

/// A photo of the last search as a `data:` URL (never a cloud placeholder, never
/// opened with another program; `None` when it cannot be shown): a 320-pixel PNG
/// preview for the grid, or with `large` the file as it is up to 8 MiB and a
/// 1600-pixel preview above that. The search's limits apply. The Duplicates
/// thumbnail command (`dupes_thumbnail`) only knows identical files and sends whole
/// files, too heavy for a grid of photos.
#[tauri::command]
pub async fn similar_thumbnail(
    similar: State<'_, SimilarState>,
    path: String,
    large: Option<bool>,
) -> CmdResult<Option<String>> {
    let p = PathBuf::from(&path);
    if !similar.knows(&p)? {
        return Err(err("not-found", path));
    }
    let large = large.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let info = LiveSource.info(&p).map_err(|e| err("os", e))?;
        if info.cloud || info.link || info.is_dir || info.size > similar::MAX_FILE {
            return Ok(None);
        }
        let mut data = Vec::new();
        std::io::Read::read_to_end(
            &mut std::io::Read::take(
                nfd_platform::io::open_sequential(&p).map_err(|e| err("os", e))?,
                similar::MAX_FILE + 1,
            ),
            &mut data,
        )
        .map_err(|e| err("os", e))?;
        if large && data.len() as u64 <= RAW_MAX {
            if let Some(mime) = sniff(&data) {
                return Ok(Some(format!(
                    "data:{mime};base64,{}",
                    super::dupes::base64(&data)
                )));
            }
        }
        let side = if large { LARGE_SIDE } else { GRID_SIDE };
        let preview = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            similar::decode::preview(&data, side)
        }));
        Ok(match preview {
            Ok(Ok(png)) => Some(format!(
                "data:image/png;base64,{}",
                super::dupes::base64(&png)
            )),
            _ => None,
        })
    })
    .await
    .map_err(|e| err("internal", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_options_default_to_strict_and_the_cache() {
        let plain: FindOptions = serde_json::from_str("{}").expect("json");
        assert!(plain.use_cache && !plain.personal && plain.min_size.is_none());
        assert_eq!(plain.preset, Preset::Strict);
        let mine: FindOptions = serde_json::from_str(
            r#"{"minSize":20480,"preset":"loose","personal":true,"useCache":false}"#,
        )
        .expect("json");
        assert!(mine.personal && !mine.use_cache);
        assert_eq!(mine.preset, Preset::Loose);
        assert_eq!(mine.min_size, Some(20_480));
    }

    #[test]
    fn selections_need_an_explicit_keep_none() {
        let s: SimilarSelection =
            serde_json::from_str(r#"{"group":3,"remove":["C:/a.jpg"]}"#).expect("json");
        assert!(!s.keep_none);
        let s: SimilarSelection =
            serde_json::from_str(r#"{"group":3,"remove":[],"keepNone":true}"#).expect("json");
        assert!(s.keep_none);
    }

    #[test]
    fn pictures_are_recognised_by_their_bytes() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some("image/png"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff(b"<svg>"), None);
    }
}
