//! macOS protected locations (docs/MACOS.md §3).
//!
//! The templates use variables that only resolve on macOS: `${MACOS_ROOT}` (`/`),
//! `${MACOS_HOME}` (the home folder) and `${MACOS_TEMP_REAL}` (the temporary folder
//! with `/var` resolved to `/private/var`). On Windows they never resolve, so none
//! of this applies there. The gate turns the package rules on when `MACOS_ROOT`
//! resolves.
//!
//! * [`MACOS_SYSTEM_TEMPLATES`] — the system's own top-level folders. Only the
//!   user's temporary folder inside them stays open ([`MACOS_CARVE_OUTS`]), as
//!   `%TEMP%` does on Windows.
//! * [`MACOS_PROTECTED_TEMPLATES`] — the user's iCloud Drive, mail, messages,
//!   keychains, app sandboxes, iPhone backups, the Trash and the media libraries.
//! * Packages ([`PACKAGE_EXTENSIONS`]): an `.app` bundle, a framework, an iWork
//!   document… is one unit — nothing *inside* it is ever deleted or deduplicated.
//!   The libraries ([`LIBRARY_EXTENSIONS`]: Photos, Music, TV, iMovie, disk image
//!   bundles…) are not deleted as a whole either.

/// Top-level system folders of the startup disk (subtree protected).
pub const MACOS_SYSTEM_TEMPLATES: &[&str] = &[
    "${MACOS_ROOT}/System",
    "${MACOS_ROOT}/Library",
    "${MACOS_ROOT}/Applications",
    "${MACOS_ROOT}/usr",
    "${MACOS_ROOT}/bin",
    "${MACOS_ROOT}/sbin",
    "${MACOS_ROOT}/opt",
    "${MACOS_ROOT}/private",
    "${MACOS_ROOT}/etc",
    "${MACOS_ROOT}/var",
    "${MACOS_ROOT}/tmp",
    "${MACOS_ROOT}/cores",
    "${MACOS_ROOT}/dev",
    "${MACOS_ROOT}/Network",
    "${MACOS_ROOT}/home",
    "${MACOS_ROOT}/net",
    "${MACOS_ROOT}/.vol",
    "${MACOS_ROOT}/.file",
];

/// Places inside [`MACOS_SYSTEM_TEMPLATES`] whose *contents* stay deletable: the
/// user's temporary folder, in both of its spellings (`/var/folders/…/T` and
/// `/private/var/folders/…/T`). The folders themselves are containers.
pub const MACOS_CARVE_OUTS: &[&str] = &["${TEMP}", "${MACOS_TEMP_REAL}"];

/// The user's data that macOS apps manage (subtree protected for every origin).
pub const MACOS_PROTECTED_TEMPLATES: &[&str] = &[
    // iCloud Drive: deleting here deletes it on every device
    "${MACOS_HOME}/Library/Mobile Documents",
    "${MACOS_HOME}/Library/Application Support/CloudDocs",
    "${MACOS_HOME}/Library/Application Support/FileProvider",
    // mail, messages, contacts, calendars, passwords, browser data
    "${MACOS_HOME}/Library/Mail",
    "${MACOS_HOME}/Library/Messages",
    "${MACOS_HOME}/Library/Keychains",
    "${MACOS_HOME}/Library/Accounts",
    "${MACOS_HOME}/Library/Calendars",
    "${MACOS_HOME}/Library/Safari",
    "${MACOS_HOME}/Library/Cookies",
    "${MACOS_HOME}/Library/HomeKit",
    "${MACOS_HOME}/Library/IdentityServices",
    "${MACOS_HOME}/Library/Preferences",
    "${MACOS_HOME}/Library/Photos",
    "${MACOS_HOME}/Library/Application Support/AddressBook",
    "${MACOS_HOME}/Library/Application Support/CallHistoryDB",
    "${MACOS_HOME}/Library/Application Support/com.apple.TCC",
    // iPhone / iPad backups
    "${MACOS_HOME}/Library/Application Support/MobileSync",
    // sandboxed apps keep all of their data here
    "${MACOS_HOME}/Library/Containers",
    "${MACOS_HOME}/Library/Group Containers",
    "${MACOS_HOME}/Library/Daemon Containers",
    // the Trash itself
    "${MACOS_HOME}/.Trash",
    // media libraries managed by Music, TV and Photo Booth
    "${MACOS_HOME}/Music/Music",
    "${MACOS_HOME}/Music/iTunes",
    "${MACOS_HOME}/Movies/TV",
    "${MACOS_HOME}/Pictures/Photo Booth Library",
];

/// The user's own files on macOS, like [`super::USER_DATA_TEMPLATES`]: never touched
/// by a rule, items inside may be picked by hand.
pub const MACOS_USER_DATA_TEMPLATES: &[&str] = &[
    "${MACOS_HOME}/Movies",
    "${MACOS_HOME}/Public",
    // Dropbox, OneDrive, Google Drive… (File Provider): deleting deletes in the cloud
    "${MACOS_HOME}/Library/CloudStorage",
];

/// Folders whose contents may be deleted by hand but which must never be deleted
/// themselves.
pub const MACOS_CONTAINER_TEMPLATES: &[&str] = &[
    "${MACOS_HOME}/Library",
    "${MACOS_HOME}/Applications",
    "${MACOS_HOME}/Library/Developer",
    "${MACOS_TEMP_REAL}",
];

/// Names protected (with everything below them) at the root of the startup disk
/// and of every volume under `/Volumes`.
pub const MACOS_VOLUME_ROOT_PROTECTED: &[&str] = &[
    ".spotlight-v100",
    ".fseventsd",
    ".trashes",
    ".documentrevisions-v100",
    ".temporaryitems",
    ".timemachine",
    ".mobilebackups",
    ".mobilebackups.trash",
    "backups.backupdb",
    ".preboot",
    ".vol",
];

/// Folders macOS shows and handles as one file (lower case, no dot): nothing inside
/// is ever deleted or deduplicated.
pub const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "appex",
    "bundle",
    "framework",
    "plugin",
    "kext",
    "xpc",
    "prefpane",
    "qlgenerator",
    "mdimporter",
    "saver",
    "component",
    "vst",
    "vst3",
    "dsym",
    "pkg",
    "mpkg",
    "rtfd",
    "pages",
    "numbers",
    "key",
    "scptd",
    "xcodeproj",
    "xcworkspace",
    "xcarchive",
    "playground",
    "docset",
    "download",
    "lrdata",
    "logicx",
    "band",
    "photoboothlibrary",
];

/// Libraries that are one unit and are not deleted here even as a whole (open the
/// app that owns them instead).
pub const LIBRARY_EXTENSIONS: &[&str] = &[
    "photoslibrary",
    "photolibrary",
    "aplibrary",
    "musiclibrary",
    "tvlibrary",
    "imovielibrary",
    "theater",
    "fcpbundle",
    "lrlibrary",
    "sparsebundle",
    "sparseimage",
    "backupbundle",
];

/// Lower-case extension of one path component (`"x.app"` → `"app"`).
fn ext(name: &str) -> Option<&str> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty()).then_some(ext)
}

/// The package or library rule `parts` (the lower-case components of a path)
/// falls under, as a template for the message.
pub(crate) fn package_rule(parts: &[&str]) -> Option<String> {
    for (i, part) in parts.iter().enumerate() {
        let Some(e) = ext(part) else { continue };
        if LIBRARY_EXTENSIONS.contains(&e) {
            return Some(format!("<library>/*.{e}"));
        }
        if i + 1 < parts.len() && PACKAGE_EXTENSIONS.contains(&e) {
            return Some(format!("<package>/*.{e}"));
        }
    }
    None
}

/// The template of [`MACOS_VOLUME_ROOT_PROTECTED`] `parts` lies in, if any:
/// `/<name>` on the startup disk, `/Volumes/<volume>/<name>` elsewhere.
pub(crate) fn volume_root_rule(parts: &[&str]) -> Option<String> {
    let top = if parts.first() == Some(&"volumes") {
        parts.get(2)?
    } else {
        parts.first()?
    };
    (MACOS_VOLUME_ROOT_PROTECTED.contains(top) || super::VOLUME_ROOT_PROTECTED.contains(top))
        .then(|| format!("<volume>/{top}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(p: &str) -> Vec<&str> {
        p.split('/').filter(|s| !s.is_empty()).collect()
    }

    #[test]
    fn packages_are_one_unit() {
        assert_eq!(package_rule(&parts("/users/a/downloads/old.app")), None);
        assert_eq!(
            package_rule(&parts("/users/a/downloads/old.app/contents/info.plist")).as_deref(),
            Some("<package>/*.app")
        );
        assert_eq!(
            package_rule(&parts("/users/a/pictures/photos library.photoslibrary")).as_deref(),
            Some("<library>/*.photoslibrary")
        );
        assert!(package_rule(&parts("/users/a/x.photoslibrary/originals/1.heic")).is_some());
        assert!(package_rule(&parts("/users/a/report.pages/data/1.png")).is_some());
        // a dot file or a plain name is no package
        assert_eq!(package_rule(&parts("/users/a/.app/x")), None);
        assert_eq!(package_rule(&parts("/users/a/app/x")), None);
    }

    #[test]
    fn volume_roots() {
        assert!(volume_root_rule(&parts("/volumes/usb/.spotlight-v100/store")).is_some());
        assert!(volume_root_rule(&parts("/volumes/tm/backups.backupdb")).is_some());
        assert!(volume_root_rule(&parts("/.fseventsd")).is_some());
        assert!(volume_root_rule(&parts("/volumes/bootcamp/windows/system32")).is_some());
        assert_eq!(volume_root_rule(&parts("/volumes/usb/movies")), None);
        assert_eq!(volume_root_rule(&parts("/volumes")), None);
    }
}
