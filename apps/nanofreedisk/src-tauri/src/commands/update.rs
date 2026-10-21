//! Settings → Updates (`docs/PHASE8_INSTALLERS.md` §2.5). Checked only when the user
//! asks. The release's `latest.json` is signed with the minisign key whose public
//! half is in `tauri.conf.json`; the plugin refuses anything else.
//!
//! Every kind of copy updates with one click, each from its own package so it
//! never ends up with a second installation elsewhere:
//! - NSIS / MSI (and the macOS app): the plugin installs the new version.
//! - Branded setup (`install.manifest` next to the exe): the new branded setup runs
//!   with `--update`, waits for the app to close, installs over this folder with the
//!   same options and starts the app again.
//! - Portable (`portable.flag`): the new zip is unpacked next to the exe; the files
//!   in use are renamed to `*.nfd-old` first (Windows allows renaming a running exe)
//!   and removed at the next start. `data` is never touched.
//!
//! The setup and the zip are verified with the same signature as the installers.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

use super::{CmdResult, err};

/// Release page (also the fallback when an update cannot be installed).
pub const RELEASES: &str = "https://github.com/congluuit/NANOFreeDisk/releases/latest";

/// `latest.json` platforms of the branded setup and the portable zip.
const SETUP_TARGET: &str = "windows-x86_64-setup";
const PORTABLE_TARGET: &str = "windows-x86_64-portable";
/// Suffix of the files a portable update replaced (removed at the next start).
const OLD_SUFFIX: &str = ".nfd-old";
/// Temp folders of downloaded updates: `%TEMP%\nfd-update-<version>`.
const TEMP_PREFIX: &str = "nfd-update-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallKind {
    /// NSIS or MSI (or the macOS app): the updater installs the new version.
    Installer,
    /// Branded setup: the new setup updates this folder.
    Setup,
    Portable,
}

fn install_kind() -> InstallKind {
    if nfd_history::portable_dir().is_some() {
        return InstallKind::Portable;
    }
    let branded = exe_dir().is_some_and(|d| d.join("install.manifest").is_file());
    if branded {
        InstallKind::Setup
    } else {
        InstallKind::Installer
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current: String,
    /// Newer version, if any.
    available: Option<String>,
    notes: Option<String>,
    date: Option<String>,
    kind: InstallKind,
    releases: &'static str,
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> CmdResult<UpdateInfo> {
    let updater = app.updater().map_err(|e| err("update", e))?;
    let update = updater.check().await.map_err(|e| err("update", e))?;
    Ok(UpdateInfo {
        current: app.package_info().version.to_string(),
        available: update.as_ref().map(|u| u.version.clone()),
        notes: update.as_ref().and_then(|u| u.body.clone()),
        date: update.as_ref().and_then(|u| u.date.map(|d| d.to_string())),
        kind: install_kind(),
        releases: RELEASES,
    })
}

/// Download progress for Settings: `{ done, total }` in bytes (`total` may be null).
#[derive(Debug, Clone, Serialize)]
struct Progress {
    done: u64,
    total: Option<u64>,
}

/// The update for this kind of copy (`None`: already up to date).
async fn find(app: &AppHandle, kind: InstallKind) -> CmdResult<Option<Update>> {
    let mut builder = app.updater_builder();
    match kind {
        InstallKind::Installer => {}
        InstallKind::Setup => builder = builder.target(SETUP_TARGET),
        InstallKind::Portable => builder = builder.target(PORTABLE_TARGET),
    }
    let updater = builder.build().map_err(|e| err("update", e))?;
    updater.check().await.map_err(|e| err("update", e))
}

/// Download and verify the update's package, reporting progress.
async fn download(app: &AppHandle, update: &Update) -> CmdResult<Vec<u8>> {
    let mut done = 0u64;
    let progress = app.clone();
    update
        .download(
            move |chunk, total| {
                done += chunk as u64;
                let _ = progress.emit("update-progress", Progress { done, total });
            },
            || {},
        )
        .await
        .map_err(|e| err("update", e))
}

/// Download, verify and install the new version, then start it.
#[tauri::command]
pub async fn update_install(app: AppHandle, lang: Option<String>) -> CmdResult<()> {
    let kind = install_kind();
    let Some(update) = find(&app, kind).await? else {
        return Ok(());
    };
    match kind {
        InstallKind::Installer => {
            let mut done = 0u64;
            let progress = app.clone();
            update
                .download_and_install(
                    move |chunk, total| {
                        done += chunk as u64;
                        let _ = progress.emit("update-progress", Progress { done, total });
                    },
                    || {},
                )
                .await
                .map_err(|e| err("update", e))?;
            app.restart();
        }
        InstallKind::Setup => {
            let bytes = download(&app, &update).await?;
            let dir = exe_dir().ok_or_else(|| err("update", "install folder unknown"))?;
            let tmp = temp_dir(&update.version)?;
            let setup = tmp.join(format!("NANOFreeDisk-Setup-{}-x64.exe", update.version));
            std::fs::write(&setup, bytes).map_err(|e| err("io", e))?;
            run_setup(&setup, &dir, lang.as_deref())?;
            app.exit(0);
        }
        InstallKind::Portable => {
            let bytes = download(&app, &update).await?;
            let dir = exe_dir().ok_or_else(|| err("update", "portable folder unknown"))?;
            let tmp = temp_dir(&update.version)?;
            let zip = tmp.join("portable.zip");
            std::fs::write(&zip, bytes).map_err(|e| err("io", e))?;
            let staged = tmp.join("files");
            unzip(&zip, &staged)?;
            replace_files(&staged, &dir).map_err(|e| err("io", e))?;
            restart_later(&dir.join(APP_EXE));
            app.exit(0);
        }
    }
    Ok(())
}

const APP_EXE: &str = "nanofreedisk.exe";

/// A fresh `%TEMP%\nfd-update-<version>`.
fn temp_dir(version: &str) -> CmdResult<PathBuf> {
    let safe: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        .collect();
    let dir = std::env::temp_dir().join(format!("{TEMP_PREFIX}{safe}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| err("io", e))?;
    Ok(dir)
}

/// Start the downloaded branded setup in update mode; it waits for this process.
#[cfg(windows)]
fn run_setup(setup: &Path, dir: &Path, lang: Option<&str>) -> CmdResult<()> {
    let mut cmd = std::process::Command::new(setup);
    cmd.arg("--update")
        .arg("--relaunch")
        .arg(format!("--wait-pid={}", std::process::id()));
    if let Some(l) = lang {
        cmd.arg(format!("/LANG={l}"));
    }
    // last: everything after /D= is the folder
    cmd.arg(format!("/D={}", dir.display()));
    cmd.current_dir(std::env::temp_dir());
    cmd.spawn().map(drop).map_err(|e| err("update", e))
}

#[cfg(not(windows))]
fn run_setup(_: &Path, _: &Path, _: Option<&str>) -> CmdResult<()> {
    Err(err("update", "the branded setup is Windows-only"))
}

/// Unpack `zip` into `to` with Windows' own `tar.exe` (Windows 10 1803+), so the
/// app needs no zip library of its own.
#[cfg(windows)]
fn unzip(zip: &Path, to: &Path) -> CmdResult<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::fs::create_dir_all(to).map_err(|e| err("io", e))?;
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let tar = PathBuf::from(root).join("System32").join("tar.exe");
    let status = std::process::Command::new(&tar)
        .arg("-xf")
        .arg(zip)
        .arg("-C")
        .arg(to)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| err("update", format!("{}: {e}", tar.display())))?;
    if status.success() {
        Ok(())
    } else {
        Err(err(
            "update",
            format!("tar.exe could not unpack the update ({status})"),
        ))
    }
}

#[cfg(not(windows))]
fn unzip(_: &Path, _: &Path) -> CmdResult<()> {
    Err(err("update", "portable updates are Windows-only"))
}

/// Every file under `dir`, as paths relative to it, except those in its `data`
/// folder (portable settings, history, quarantine).
fn files_in(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for e in std::fs::read_dir(dir.join(&rel))? {
            let e = e?;
            let path = rel.join(e.file_name());
            if e.file_type()?.is_dir() {
                if !in_data(&path) {
                    stack.push(path);
                }
            } else {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn old_name(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(OLD_SUFFIX);
    PathBuf::from(s)
}

/// Copy the files of `staged` over `dir`. A file that exists is renamed to
/// `*.nfd-old` first, so files in use (the running exe) can be replaced; on any
/// error everything done so far is put back. The `data` folder is never written.
fn replace_files(staged: &Path, dir: &Path) -> std::io::Result<()> {
    let files = files_in(staged)?;
    if !files
        .iter()
        .any(|f| f.as_os_str().eq_ignore_ascii_case(APP_EXE))
    {
        return Err(std::io::Error::other(format!(
            "{APP_EXE} is missing from the update"
        )));
    }
    let mut done = Vec::new();
    let result = copy_over(&files, staged, dir, &mut done);
    if result.is_err() {
        for (target, old) in done.into_iter().rev() {
            let _ = std::fs::remove_file(&target);
            if let Some(old) = old {
                let _ = std::fs::rename(&old, &target);
            }
        }
    }
    result
}

fn in_data(rel: &Path) -> bool {
    rel.components()
        .next()
        .is_some_and(|c| c.as_os_str().eq_ignore_ascii_case("data"))
}

/// The copy step of [`replace_files`]; `done` lists each target written and the
/// `*.nfd-old` it was moved to, for the roll-back.
fn copy_over(
    files: &[PathBuf],
    staged: &Path,
    dir: &Path,
    done: &mut Vec<(PathBuf, Option<PathBuf>)>,
) -> std::io::Result<()> {
    for rel in files.iter().filter(|r| !in_data(r)) {
        let target = dir.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let old = if target.exists() {
            let old = old_name(&target);
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&target, &old)?;
            Some(old)
        } else {
            None
        };
        done.push((target.clone(), old));
        std::fs::copy(staged.join(rel), &target)?;
    }
    Ok(())
}

/// Start `exe` two seconds from now, after this instance has closed (otherwise the
/// single-instance check would hand the start over to this window and quit).
#[cfg(windows)]
fn restart_later(exe: &Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let script = format!(
        "ping -n 3 127.0.0.1 >nul & start \"\" \"{}\"",
        exe.display()
    );
    let _ = std::process::Command::new("cmd.exe")
        .arg("/d")
        .arg("/c")
        .raw_arg(&script)
        .current_dir(std::env::temp_dir())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn();
}

#[cfg(not(windows))]
fn restart_later(_: &Path) {}

/// At start-up: remove what an earlier update left behind — the downloads in
/// `%TEMP%` and, in a portable copy, the replaced `*.nfd-old` files.
pub fn clean_leftovers() {
    if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(TEMP_PREFIX) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    if install_kind() != InstallKind::Portable {
        return;
    }
    let Some(dir) = exe_dir() else { return };
    for rel in files_in(&dir).unwrap_or_default() {
        if !in_data(&rel) && rel.to_string_lossy().ends_with(OLD_SUFFIX) {
            let _ = std::fs::remove_file(dir.join(rel));
        }
    }
}

/// Open the latest release page.
#[tauri::command]
pub fn update_open_releases(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(RELEASES, None::<&str>)
        .map_err(|e| err("os", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_files_keeps_the_old_copy_and_never_writes_data() {
        let staged = tempfile::tempdir().expect("tmp");
        let dir = tempfile::tempdir().expect("tmp");
        let (s, d) = (staged.path(), dir.path());
        std::fs::write(s.join(APP_EXE), b"new").expect("write");
        std::fs::create_dir_all(s.join("rules")).expect("mkdir");
        std::fs::write(s.join("rules").join("a.toml"), b"rule").expect("write");
        std::fs::create_dir_all(s.join("data")).expect("mkdir");
        std::fs::write(s.join("data").join("x"), b"from zip").expect("write");
        std::fs::write(d.join(APP_EXE), b"old").expect("write");
        std::fs::create_dir_all(d.join("data")).expect("mkdir");
        std::fs::write(d.join("data").join("x"), b"mine").expect("write");

        replace_files(s, d).expect("replace");
        assert_eq!(std::fs::read(d.join(APP_EXE)).expect("read"), b"new");
        assert_eq!(
            std::fs::read(old_name(&d.join(APP_EXE))).expect("read"),
            b"old"
        );
        assert_eq!(
            std::fs::read(d.join("rules").join("a.toml")).expect("read"),
            b"rule"
        );
        assert_eq!(
            std::fs::read(d.join("data").join("x")).expect("read"),
            b"mine"
        );
    }

    #[test]
    fn replace_files_refuses_a_package_without_the_app() {
        let staged = tempfile::tempdir().expect("tmp");
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(staged.path().join("README.md"), b"x").expect("write");
        assert!(replace_files(staged.path(), dir.path()).is_err());
        assert!(!dir.path().join("README.md").exists());
    }
}
