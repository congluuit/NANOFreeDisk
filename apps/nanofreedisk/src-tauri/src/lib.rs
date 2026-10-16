//! NANOFreeDisk desktop app (Tauri 2). The UI lives in `../src` (Svelte 5); this
//! crate exposes the scanner and the safe Recycle Bin action as commands.
//!
//! Nothing here deletes directly: [`commands::recycle`] re-runs the checks of
//! `nfd_core::safety` against the live filesystem for every item first.

mod commands;
mod session;

use tauri::{Emitter, Manager};

/// The folder to analyze from the command line (`nanofreedisk.exe "D:\\Data"`, which
/// is what the Explorer menu runs): the first argument that is not a flag. Relative
/// paths are resolved against `cwd`.
fn path_argument(args: &[String], cwd: &str) -> Option<String> {
    let arg = args.iter().skip(1).find(|a| !a.starts_with('-'))?;
    let path = std::path::Path::new(arg.trim_matches('"'));
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::path::Path::new(cwd).join(path)
    };
    Some(full.to_string_lossy().into_owned())
}

/// Start the application.
///
/// `--unregister-shell` removes the Explorer menu keys and exits (for uninstallers).
///
/// # Panics
/// If the Tauri runtime cannot start (no WebView2, broken bundle).
#[allow(clippy::expect_used)]
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--unregister-shell") {
        let _ = nfd_platform::shell_menu::unregister();
        return;
    }
    let cwd = std::env::current_dir()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    let state = commands::AppState::default();
    state.set_startup_path(path_argument(&args, &cwd));

    tauri::Builder::default()
        // must come first: a second launch hands its folder to this window and exits
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            if let Some(path) = path_argument(&argv, &cwd) {
                let _ = app.emit("open-path", path);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .manage(commands::hwmon::HwState::default())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_volumes,
            commands::start_scan,
            commands::cancel_scan,
            commands::summary,
            commands::issues,
            commands::children,
            commands::detail,
            commands::slice,
            commands::top_files,
            commands::extensions,
            commands::search,
            commands::old_and_large,
            commands::empty_folders,
            commands::broken_shortcuts,
            commands::history_snapshots,
            commands::history_diff,
            commands::history_info,
            commands::history_clear,
            commands::history_forget,
            commands::find_path,
            commands::explain,
            commands::take_startup_path,
            commands::shell_menu_status,
            commands::shell_menu_set,
            commands::age_matrix,
            commands::owner,
            commands::broken_links,
            commands::health,
            commands::reveal,
            commands::open_item,
            commands::plan_delete,
            commands::recycle,
            commands::export_report,
            commands::save_html_report,
            commands::load_settings,
            commands::save_settings,
            commands::read_text_file,
            commands::write_text_file,
            commands::hwmon::hw_inventory,
            commands::hwmon::hw_export,
            commands::hwmon::sensors_snapshot,
            commands::hwmon::sensors_reset_minmax,
            commands::hwmon::sensors_history,
            commands::hwmon::tasks_snapshot,
            commands::hwmon::task_verdict,
            commands::hwmon::task_details,
            commands::hwmon::task_end,
            commands::hwmon::task_reveal,
            commands::hwmon::open_task_manager,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NANOFreeDisk");
}

#[cfg(test)]
mod tests {
    use super::path_argument;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn path_from_command_line() {
        assert_eq!(path_argument(&args(&["app.exe"]), "C:\\"), None);
        assert_eq!(path_argument(&args(&["app.exe", "--flag"]), "C:\\"), None);
        let abs = if cfg!(windows) { "D:\\Data" } else { "/data" };
        assert_eq!(
            path_argument(&args(&["app.exe", abs]), "C:\\").as_deref(),
            Some(abs)
        );
        let rel = path_argument(&args(&["app.exe", "sub"]), "base").expect("relative");
        assert!(rel.starts_with("base") && rel.ends_with("sub"), "{rel}");
    }
}
