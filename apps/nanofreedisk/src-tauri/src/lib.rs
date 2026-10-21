//! NANOFreeDisk desktop app (Tauri 2). The UI lives in `../src` (Svelte 5); this
//! crate exposes the scanner, the Clean and DevCache views and the Activity log as
//! commands.
//!
//! Nothing here deletes directly: every delete goes through
//! `Planner → SafetyGate → Executor → UndoJournal` ([`commands::clean`]).

mod commands;
mod paths;
mod session;
mod tray;
mod uninstall;

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

/// "Find duplicates here" (`--dupes <folder>`) rather than "Analyze".
fn wants_dupes(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .any(|a| a == nfd_platform::shell_menu::DUPES_FLAG)
}

/// Start the application.
///
/// `--unregister-shell` removes the Explorer menu keys and exits (for uninstallers);
/// `--uninstall-cleanup` is the uninstallers' data step ([`uninstall`]).
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
    // run by the uninstallers: quarantine restore/purge, data removal (no window)
    if args.iter().any(|a| a == uninstall::FLAG) {
        std::process::exit(uninstall::run(&args));
    }
    // the uninstaller's one elevated step (all-users leftovers): no window, one request file
    if args.iter().any(|a| a == nfd_apps::elevated::FLAG) {
        std::process::exit(nfd_apps::elevated::run(&args));
    }
    // elevated helper (Advanced sensors, Monitor detail): no window, only the pipe protocol
    #[cfg(windows)]
    if args.iter().any(|a| a == nfd_helper::HELPER_FLAG) {
        std::process::exit(nfd_helper::run_helper(&args));
    }
    let cwd = std::env::current_dir()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    let state = commands::AppState::default();
    if wants_dupes(&args) {
        state.set_startup_dupes(path_argument(&args, &cwd));
    } else {
        state.set_startup_path(path_argument(&args, &cwd));
    }

    tauri::Builder::default()
        // must come first: a second launch hands its folder to this window and exits
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            if let Some(path) = path_argument(&argv, &cwd) {
                let event = if wants_dupes(&argv) {
                    "open-dupes"
                } else {
                    "open-path"
                };
                let _ = app.emit(event, path);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .manage(commands::clean::CleanState::default())
        .manage(commands::devcache::DevState::default())
        .manage(commands::ai_leftovers::AiState::default())
        .manage(commands::advice::AdviceState::default())
        .manage(commands::browsers::BrowserState::default())
        .manage(commands::dupes::DupeState::default())
        .manage(commands::hwmon::HwState::default())
        .manage(commands::monitor::MonitorState::default())
        .manage(commands::diskhealth::HealthState::default())
        .manage(commands::programs::ProgramsState::default())
        .setup(|app| {
            // the main window comes from tauri.conf.json ("create": false) so
            // portable mode can keep the WebView2 profile next to the exe
            paths::main_window(app.handle())?;
            // the notification-area icon (G5); the app works without it
            let _ = tray::create(app.handle());
            Ok(())
        })
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
            commands::clean::clean_scan,
            commands::clean::clean_items,
            commands::clean::clean_run,
            commands::clean::clean_cancel,
            commands::clean::delete_entries,
            commands::clean::activity_list,
            commands::clean::activity_items,
            commands::clean::activity_restore,
            commands::clean::activity_purge,
            commands::clean::activity_forget,
            commands::devcache::devcache_scan,
            commands::devcache::devcache_items,
            commands::devcache::devcache_run,
            commands::devcache::devcache_cancel,
            commands::devcache::devcache_unknown,
            commands::devcache::devcache_toolchains,
            commands::devcache::devcache_repos,
            commands::devcache::devcache_repos_clean,
            commands::devcache::devcache_stores,
            commands::devcache::devcache_store_run,
            commands::devcache::devcache_reveal,
            commands::devcache::devcache_propose,
            commands::devcache::devcache_notify,
            commands::advice::advice_summary,
            commands::advice::advice_detail,
            commands::advice::advice_refresh,
            commands::advice::advice_clean,
            commands::advice::advice_cancel,
            commands::advice::advice_override,
            commands::ai_leftovers::ai_leftovers_scan,
            commands::ai_leftovers::ai_leftovers_plan,
            commands::ai_leftovers::ai_leftovers_clean,
            commands::browsers::browsers_sites,
            commands::browsers::browsers_plan,
            commands::browsers::browsers_clean,
            commands::browsers::browsers_cancel,
            commands::dupes::dupes_find,
            commands::dupes::dupes_cancel,
            commands::dupes::dupes_report,
            commands::dupes::dupes_select,
            commands::dupes::dupes_run,
            commands::dupes::dupes_reveal,
            commands::dupes::dupes_thumbnail,
            commands::dupes::dupes_export,
            commands::take_startup_dupes,
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
            commands::hwmon::pawnio_status,
            commands::hwmon::advanced_sensors_start,
            commands::hwmon::pawnio_setup,
            commands::hwmon::pawnio_open_site,
            commands::hwmon::thermal_set,
            commands::hwmon::thermal_status,
            commands::update::update_check,
            commands::update::update_install,
            commands::update::update_open_releases,
            commands::monitor::monitor_snapshot,
            commands::monitor::monitor_chart,
            commands::monitor::monitor_pause,
            commands::monitor::monitor_detail,
            commands::monitor::monitor_watch,
            commands::monitor::monitor_configure,
            commands::monitor::monitor_history,
            commands::monitor::monitor_reveal,
            tray::tray_monitor_text,
            commands::diskhealth::health_report,
            commands::diskhealth::health_elevated,
            commands::diskhealth::health_export,
            commands::diskhealth::health_watch_set,
            commands::programs::programs_list,
            commands::programs::programs_uninstall,
            commands::programs::programs_stop_waiting,
            commands::programs::programs_leftovers,
            commands::programs::programs_leftovers_remove,
            commands::programs::programs_junk_scan,
            commands::programs::programs_junk_clean,
            commands::programs::programs_reveal,
            commands::programs::activity_restore_admin,
        ])
        .build(tauri::generate_context!())
        .expect("error while running NANOFreeDisk")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                commands::hwmon::thermal_flush(app);
                commands::monitor::monitor_flush(app);
            }
        });
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
        let dupes = args(&["app.exe", "--dupes", abs]);
        assert!(super::wants_dupes(&dupes));
        assert!(!super::wants_dupes(&args(&["app.exe", abs])));
        assert_eq!(path_argument(&dupes, "C:\\").as_deref(), Some(abs));
    }
}
