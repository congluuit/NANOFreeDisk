//! Where the app keeps its data. Installed: the per-user folders Tauri derives
//! from the app identifier (`%LOCALAPPDATA%\vn.io.nanodev.nanofreedisk`,
//! `%APPDATA%\vn.io.nanodev.nanofreedisk`). Portable (`portable.flag` next to the
//! exe, see [`nfd_history::portable_dir`]): one `data` folder beside the exe, so
//! nothing is written to the user profile. Every data path goes through here.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};

/// Local data: history, hash cache, quarantine of the system drive, logs.
pub fn local_data<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    nfd_history::portable_dir().or_else(|| app.path().app_local_data_dir().ok())
}

/// Roaming data (24 h temperature history).
pub fn roaming_data<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    nfd_history::portable_dir().or_else(|| app.path().app_data_dir().ok())
}

/// Settings.
pub fn config<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    nfd_history::portable_dir().or_else(|| app.path().app_config_dir().ok())
}

/// Logs (audit log, End task log).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn logs<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    nfd_history::portable_dir()
        .map(|d| d.join("logs"))
        .or_else(|| app.path().app_log_dir().ok())
}

/// Build the main window from `tauri.conf.json`. Tauri would put the WebView2
/// profile in `%LOCALAPPDATA%\<identifier>`; portable mode keeps it in `data`.
///
/// # Errors
/// The window could not be created.
pub fn main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let Some(config) = app.config().app.windows.iter().find(|w| w.label == "main") else {
        return Ok(());
    };
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, config)?;
    if let Some(dir) = nfd_history::portable_dir() {
        builder = builder.data_directory(dir);
    }
    builder.build()?;
    Ok(())
}
