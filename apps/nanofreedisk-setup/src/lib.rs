//! NANOFreeDisk branded setup (`docs/PHASE8_INSTALLERS.md` §2.4): a small Rust/egui
//! installer that does not need WebView2 itself. The library holds everything that
//! can be tested without a window: the payload format, the command line, the texts
//! and (on Windows) the install / uninstall steps.

pub mod args;
pub mod payload;
pub mod text;

#[cfg(windows)]
pub mod install;
#[cfg(windows)]
pub mod sys;

/// Product name: install folder, registry key, shortcut name.
pub const PRODUCT: &str = "NANOFreeDisk";
/// Main executable inside the install folder.
pub const APP_EXE: &str = "nanofreedisk.exe";
/// Publisher shown in Apps & features.
pub const PUBLISHER: &str = "NANO (nanodev.io.vn)";
/// Version of the files this setup installs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Homepage / help link.
pub const HOMEPAGE: &str = "https://nanodev.io.vn/";
/// Releases page (updates for this kind of install).
pub const RELEASES: &str = "https://github.com/congluuit/NANOFreeDisk/releases";
