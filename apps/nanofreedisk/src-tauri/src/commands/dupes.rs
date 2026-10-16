//! Duplicates view (PHASE5_DUPLICATES §2.6): byte-identical files in the current
//! scan or in folders the user picks, the copy to keep per group, and removal of the
//! others the Phase 3 way — `Planner → SafetyGate → Executor → UndoJournal`
//! ([`super::clean::execute_plan_verified`]) with every copy and its keeper hashed
//! again right before the move.
//!
//! Folders searched here are scanned on their own: the Analyze scan and the scan
//! history are left alone. Hashes are cached in the history database (unless the
//! caller opts out, as the GUI smoke test does). Only paths of the last search can
//! be revealed in Explorer, previewed or acted on.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use nfd_core::safety::SafetyGate;
use nfd_core::scanner::{self, ScanEvent, ScanOptions, ScanTree};
use nfd_dupes::cache::HashCache;
use nfd_dupes::hash::{LiveSource, Source};
use nfd_dupes::keep::{KeepStrategy, Selection, auto_select};
use nfd_dupes::plan::{Action, Verifier};
use nfd_dupes::{DupeOptions, DupeReport, Phase, Progress};
use nfd_platform::Platform;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use super::clean::{self, OperationDto};
use super::{AppState, CmdResult, err};

/// Groups sent to the UI at most (largest waste first); the rest stay counted.
const MAX_GROUPS: usize = 5_000;
/// Largest image previewed inline.
const THUMB_MAX: u64 = 6 << 20;
const THUMB_EXTS: &[(&str, &str)] = &[
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("png", "image/png"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
];

/// The last search and the running one, if any.
#[derive(Debug, Default)]
pub struct DupeState {
    report: Mutex<Option<DupeReport>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

fn key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

impl DupeState {
    /// (wasted bytes, groups, first folder) of the last search, for the Health card.
    pub fn summary(&self) -> Option<(u64, usize, String)> {
        let guard = self.report.lock().ok()?;
        guard.as_ref().map(|r| {
            (
                r.wasted_bytes,
                r.groups.len(),
                r.roots
                    .first()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )
        })
    }

    /// `true` when `path` is a copy (or a link of one) in the last search.
    fn knows(&self, path: &Path) -> CmdResult<bool> {
        let k = key(path);
        let guard = self.report.lock().map_err(|e| err("internal", e))?;
        Ok(guard.as_ref().is_some_and(|r| {
            r.groups.iter().any(|g| {
                g.copies
                    .iter()
                    .any(|c| key(&c.path) == k || c.links.iter().any(|l| key(l) == k))
            })
        }))
    }
}

/// What the UI asks for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindOptions {
    min_size: u64,
    #[serde(default)]
    include_app_dirs: bool,
    #[serde(default)]
    paranoid: bool,
    #[serde(default)]
    extensions: Vec<String>,
    /// Read and write the hash cache in the history database.
    #[serde(default = "yes")]
    use_cache: bool,
}

const fn yes() -> bool {
    true
}

/// Progress event `dupes-progress`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressDto {
    /// `scan`, `collect`, `partial`, `full` or `compare`.
    phase: &'static str,
    done: u64,
    total: u64,
    bytes: u64,
    total_bytes: u64,
    path: String,
}

/// The search result as the UI gets it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeView {
    #[serde(flatten)]
    report: DupeReport,
    /// Groups found (the list holds at most [`MAX_GROUPS`]).
    total_groups: usize,
}

fn view(report: &DupeReport) -> DupeView {
    let mut shown = report.clone();
    shown.groups.truncate(MAX_GROUPS);
    DupeView {
        total_groups: report.groups.len(),
        report: shown,
    }
}

const fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Collect => "collect",
        Phase::Partial => "partial",
        Phase::Full => "full",
        Phase::Compare => "compare",
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
                    "dupes-progress",
                    ProgressDto {
                        phase: "scan",
                        done: s.files,
                        total: 0,
                        bytes: s.logical,
                        total_bytes: 0,
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

/// Find duplicates in `paths` (folders or drives), or in the current Analyze scan
/// when `paths` is empty. Progress streams as `dupes-progress`; [`dupes_cancel`]
/// stops the search.
#[tauri::command]
pub async fn dupes_find(
    app: AppHandle,
    state: State<'_, AppState>,
    dupes: State<'_, DupeState>,
    paths: Vec<String>,
    options: FindOptions,
) -> CmdResult<DupeView> {
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
        if !Path::new(p).is_dir() {
            return Err(err("not-found", p));
        }
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *dupes.cancel.lock().map_err(|e| err("internal", e))? = Some(cancel.clone());
    let db = if options.use_cache {
        super::history_path(&app).ok()
    } else {
        None
    };
    let app2 = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> CmdResult<DupeReport> {
        let trees: Vec<ScanTree> = match current {
            Some(t) => vec![t],
            None => paths
                .iter()
                .map(|p| scan(&app2, Path::new(p), &cancel))
                .collect::<CmdResult<_>>()?,
        };
        let resolve = nfd_platform::Native.resolver();
        let gate = SafetyGate::new(&resolve);
        let opts = DupeOptions {
            min_size: options.min_size.max(1),
            include_app_dirs: options.include_app_dirs,
            paranoid: options.paranoid,
            threads: None,
            extensions: options.extensions,
            app_dir_paths: nfd_dupes::collect::default_app_dirs(&resolve),
            cancel: Some(cancel.clone()),
        };
        // a connection of its own: the app's shared one stays free meanwhile
        let mut history = db.and_then(|p| nfd_history::History::open(&p).ok());
        let refs: Vec<&ScanTree> = trees.iter().collect();
        let emit = |p: &Progress| {
            let _ = app2.emit(
                "dupes-progress",
                ProgressDto {
                    phase: phase_name(p.phase),
                    done: p.done,
                    total: p.total,
                    bytes: p.bytes,
                    total_bytes: p.total_bytes,
                    path: String::new(),
                },
            );
        };
        nfd_dupes::find(
            &refs,
            &gate,
            &opts,
            history.as_mut().map(|h| h as &mut dyn HashCache),
            &LiveSource,
            &emit,
        )
        .map_err(|_| err("cancelled", "search cancelled"))
    })
    .await
    .map_err(|e| err("internal", e))?;
    *dupes.cancel.lock().map_err(|e| err("internal", e))? = None;
    let report = result?;
    let out = view(&report);
    *dupes.report.lock().map_err(|e| err("internal", e))? = Some(report);
    Ok(out)
}

/// Stop the running search or removal.
#[tauri::command]
pub fn dupes_cancel(dupes: State<'_, DupeState>) -> CmdResult<()> {
    if let Some(c) = dupes
        .cancel
        .lock()
        .map_err(|e| err("internal", e))?
        .as_ref()
    {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// The last search again (after a removal it no longer lists removed copies).
#[tauri::command]
pub fn dupes_report(dupes: State<'_, DupeState>) -> CmdResult<Option<DupeView>> {
    let guard = dupes.report.lock().map_err(|e| err("internal", e))?;
    Ok(guard.as_ref().map(view))
}

/// The default selection of every group for `strategy` (`auto`, `oldest`, `newest`,
/// `shortest`, `prefer:<dir>`): one copy kept, risky copies never selected.
#[tauri::command]
pub fn dupes_select(dupes: State<'_, DupeState>, strategy: String) -> CmdResult<Vec<Selection>> {
    let strategy: KeepStrategy = strategy.parse().map_err(|e| err("bad-strategy", e))?;
    let guard = dupes.report.lock().map_err(|e| err("internal", e))?;
    let report = guard
        .as_ref()
        .ok_or_else(|| err("no-dupes", "no duplicate search yet"))?;
    let mut sel = auto_select(&report.groups, &strategy);
    sel.retain(|s| s.group as usize <= MAX_GROUPS);
    Ok(sel)
}

/// Remove the copies of `selections` (Recycle Bin or quarantine). The planner
/// refuses a selection that keeps nothing; each copy and its keeper are hashed again
/// right before the move. Progress streams as `clean-progress`.
#[tauri::command]
pub async fn dupes_run(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    dupes: State<'_, DupeState>,
    selections: Vec<Selection>,
    action: Action,
) -> CmdResult<OperationDto> {
    let (plan, groups) = {
        let guard = dupes.report.lock().map_err(|e| err("internal", e))?;
        let report = guard
            .as_ref()
            .ok_or_else(|| err("no-dupes", "no duplicate search yet"))?;
        let plan = nfd_dupes::plan::plan(&report.groups, &selections, action)
            .map_err(|e| err("dupes-plan", e))?;
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
    *dupes.cancel.lock().map_err(|e| err("internal", e))? = Some(cancel.clone());
    let mut verifier = Verifier::new(&LiveSource, groups);
    let (dto, op) = clean::execute_plan_returning(
        &app,
        &window,
        &state,
        plan,
        "dupes",
        cancel,
        Box::new(move |item| verifier.check(item)),
    )
    .await?;
    *dupes.cancel.lock().map_err(|e| err("internal", e))? = None;
    // forget what left the disk
    let gone: HashSet<String> = op
        .items
        .iter()
        .filter(|i| i.status.removed_something())
        .map(|i| key(&i.path))
        .collect();
    if let Some(report) = dupes
        .report
        .lock()
        .map_err(|e| err("internal", e))?
        .as_mut()
    {
        forget(report, &gone);
    }
    Ok(dto)
}

/// Drop removed copies (and groups left with one copy) from `report`.
fn forget(report: &mut DupeReport, gone: &HashSet<String>) {
    for g in &mut report.groups {
        g.copies.retain(|c| !gone.contains(&key(&c.path)));
        g.wasted = g.size * (g.copies.len().max(1) as u64 - 1);
    }
    report.groups.retain(|g| g.copies.len() > 1);
    report.wasted_bytes = report.groups.iter().map(|g| g.wasted).sum();
}

/// Show a copy of the last search in Explorer.
#[tauri::command]
pub fn dupes_reveal(app: AppHandle, dupes: State<'_, DupeState>, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = PathBuf::from(&path);
    if !dupes.knows(&p)? {
        return Err(err("not-found", path));
    }
    app.opener()
        .reveal_item_in_dir(&p)
        .map_err(|e| err("os", e))
}

/// A small image of the last search as a `data:` URL (images up to 6 MiB, never a
/// cloud placeholder); `None` for anything else. The file is read here, never opened
/// with another program.
#[tauri::command]
pub async fn dupes_thumbnail(
    dupes: State<'_, DupeState>,
    path: String,
) -> CmdResult<Option<String>> {
    let p = PathBuf::from(&path);
    if !dupes.knows(&p)? {
        return Err(err("not-found", path));
    }
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let Some((_, mime)) = THUMB_EXTS.iter().find(|(x, _)| *x == ext) else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let info = LiveSource.info(&p).map_err(|e| err("os", e))?;
        if info.cloud || info.link || info.is_dir || info.size > THUMB_MAX {
            return Ok(None);
        }
        let mut data = Vec::new();
        std::io::Read::read_to_end(
            &mut nfd_platform::io::open_sequential(&p).map_err(|e| err("os", e))?,
            &mut data,
        )
        .map_err(|e| err("os", e))?;
        Ok(Some(format!("data:{mime};base64,{}", base64(&data))))
    })
    .await
    .map_err(|e| err("internal", e))?
}

/// Save the last search as `nanofreedisk.dupes.v1` JSON or CSV (`path` must end in
/// `.json` / `.csv`).
#[tauri::command]
pub fn dupes_export(dupes: State<'_, DupeState>, path: String, format: String) -> CmdResult<()> {
    let p = PathBuf::from(&path);
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext != format || !matches!(format.as_str(), "json" | "csv") {
        return Err(err("bad-path", path));
    }
    let guard = dupes.report.lock().map_err(|e| err("internal", e))?;
    let report = guard
        .as_ref()
        .ok_or_else(|| err("no-dupes", "no duplicate search yet"))?;
    let bytes = if format == "json" {
        let mut v = serde_json::to_value(report).map_err(|e| err("internal", e))?;
        v["schema"] = nfd_dupes::SCHEMA.into();
        v["version"] = env!("CARGO_PKG_VERSION").into();
        serde_json::to_vec_pretty(&v).map_err(|e| err("internal", e))?
    } else {
        let mut out = Vec::new();
        nfd_dupes::write_csv(report, &mut out).map_err(|e| err("os", e))?;
        out
    };
    std::fs::write(&p, bytes).map_err(|e| err("os", e))
}

/// Standard base64 with padding.
fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), want);
        }
    }

    #[test]
    fn removed_copies_leave_the_report() {
        use nfd_dupes::{CopyFlags, DupeGroup, FileCopy};
        let copy = |p: &str| FileCopy {
            path: PathBuf::from(p),
            mtime: 0,
            volume: 0,
            file_id: None,
            links: Vec::new(),
            flags: CopyFlags::default(),
            link: None,
        };
        let mut r = DupeReport {
            groups: vec![
                DupeGroup {
                    id: 1,
                    hash: String::new(),
                    size: 10,
                    wasted: 20,
                    copies: vec![copy("C:/a"), copy("C:/b"), copy("C:/c")],
                },
                DupeGroup {
                    id: 2,
                    hash: String::new(),
                    size: 5,
                    wasted: 5,
                    copies: vec![copy("C:/x"), copy("C:/y")],
                },
            ],
            wasted_bytes: 25,
            ..DupeReport::default()
        };
        let gone: HashSet<String> = ["C:/b", "C:/Y"].iter().map(|p| key(Path::new(p))).collect();
        forget(&mut r, &gone);
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].copies.len(), 2);
        assert_eq!(r.wasted_bytes, 10);
    }

    #[test]
    fn thumbnails_only_for_images() {
        assert!(
            THUMB_EXTS
                .iter()
                .all(|(e, m)| m.starts_with("image/") && !e.is_empty())
        );
    }
}
