//! NANOFreeDisk desktop app (Tauri 2). The UI lives in `../src` (Svelte 5); this
//! crate exposes the scanner and the safe Recycle Bin action as commands.
//!
//! Nothing here deletes directly: [`commands::recycle`] re-runs the checks of
//! `nfd_core::safety` against the live filesystem for every item first.

mod commands;
mod session;

/// Start the application.
///
/// # Panics
/// If the Tauri runtime cannot start (no WebView2, broken bundle).
#[allow(clippy::expect_used)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::AppState::default())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running NANOFreeDisk");
}
