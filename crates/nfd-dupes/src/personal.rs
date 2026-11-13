//! The user's own folders: Desktop, Documents, Downloads, Pictures, Videos (Movies
//! on macOS) and Music, where most duplicate photos, documents and downloads live.
//! The one-click search of the Duplicates view looks there without an Analyze scan.
//!
//! Windows asks the shell for each known folder (`SHGetKnownFolderPath`), so a
//! Documents folder moved to another drive or into OneDrive is found where it really
//! is. macOS uses the fixed folders of the home directory, Linux the XDG user
//! directories (`~/.config/user-dirs.dirs`) with the usual names as fallback.

use std::path::{Path, PathBuf};

use crate::collect::key;

/// Smallest file the personal-folders search compares (100 KiB).
///
/// Phone and chat photos, screenshots, scans and office documents are mostly
/// between 100 KiB and a few MiB, so the 1 MiB of [`crate::DEFAULT_MIN_SIZE`] misses
/// many of them. Below 100 KiB live icons, thumbnails, small text and saved web page
/// parts: often most of the files, yet little space, and every candidate costs a
/// file open. The hash cache makes a second run cheap either way.
pub const PERSONAL_MIN_SIZE: u64 = 100 << 10;

/// The personal folders that exist, without folders nested in another one of the
/// list (each file is looked at once).
#[must_use]
pub fn personal_folders() -> Vec<PathBuf> {
    outermost(resolve().into_iter().filter(|p| p.is_dir()).collect())
}

/// `paths` in their order, minus repeats and paths inside another path of the list
/// (compared case-insensitively, `/` and `\` alike).
#[must_use]
pub fn outermost(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let keys: Vec<String> = paths
        .iter()
        .map(|p| key(p).trim_end_matches('/').to_owned())
        .collect();
    let inside = |k: &str, other: &str| {
        k.len() > other.len() && k.starts_with(other) && k.as_bytes()[other.len()] == b'/'
    };
    paths
        .into_iter()
        .enumerate()
        .filter(|(i, _)| {
            let k = &keys[*i];
            !keys
                .iter()
                .enumerate()
                .any(|(j, o)| (o == k && j < *i) || inside(k, o))
        })
        .map(|(_, p)| p)
        .collect()
}

#[cfg(windows)]
fn resolve() -> Vec<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music,
        FOLDERID_Pictures, FOLDERID_Videos, SHGetKnownFolderPath,
    };
    use windows_sys::core::{GUID, PWSTR};

    fn known(id: &GUID) -> Option<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        let mut p: PWSTR = std::ptr::null_mut();
        // SAFETY: `id` is a valid known folder id; the out string is freed with
        // CoTaskMemFree, also on failure (documented), and read only when non-null
        // (a NUL-terminated wide string).
        unsafe {
            let hr = SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut p);
            let out = if hr >= 0 && !p.is_null() {
                let mut n = 0;
                while *p.add(n) != 0 {
                    n += 1;
                }
                Some(PathBuf::from(std::ffi::OsString::from_wide(
                    std::slice::from_raw_parts(p, n),
                )))
            } else {
                None
            };
            CoTaskMemFree(p.cast());
            out
        }
    }

    [
        &FOLDERID_Documents,
        &FOLDERID_Downloads,
        &FOLDERID_Pictures,
        &FOLDERID_Videos,
        &FOLDERID_Music,
        &FOLDERID_Desktop,
    ]
    .into_iter()
    .filter_map(known)
    .collect()
}

#[cfg(target_os = "macos")]
fn resolve() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    [
        "Documents",
        "Downloads",
        "Pictures",
        "Movies",
        "Music",
        "Desktop",
    ]
    .iter()
    .map(|n| home.join(n))
    .collect()
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn resolve() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let text = std::fs::read_to_string(home.join(".config/user-dirs.dirs")).unwrap_or_default();
    xdg_dirs(&text, &home)
}

/// Personal folders from an XDG `user-dirs.dirs` text, the usual English names for
/// the ones it does not set. Only folders inside `home` are taken.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn xdg_dirs(text: &str, home: &Path) -> Vec<PathBuf> {
    [
        ("XDG_DOCUMENTS_DIR", "Documents"),
        ("XDG_DOWNLOAD_DIR", "Downloads"),
        ("XDG_PICTURES_DIR", "Pictures"),
        ("XDG_VIDEOS_DIR", "Videos"),
        ("XDG_MUSIC_DIR", "Music"),
        ("XDG_DESKTOP_DIR", "Desktop"),
    ]
    .iter()
    .filter_map(|(var, fallback)| {
        let set = text.lines().find_map(|l| {
            let value = l.trim().strip_prefix(var)?.trim_start().strip_prefix('=')?;
            let value = value.trim().trim_matches('"');
            if let Some(rest) = value.strip_prefix("$HOME") {
                Some(home.join(rest.trim_start_matches('/')))
            } else if value.starts_with('/') {
                Some(PathBuf::from(value))
            } else {
                None
            }
        });
        let path = set.unwrap_or_else(|| home.join(fallback));
        // `$HOME/` alone means "not set" in that file: never the whole home folder
        (path != home && path.starts_with(home)).then_some(path)
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_and_repeated_folders_are_searched_once() {
        let got = outermost(vec![
            PathBuf::from("C:/Users/a/Documents"),
            PathBuf::from("C:/Users/a/Documents/My Pictures"),
            PathBuf::from("c:\\users\\a\\documents\\"),
            PathBuf::from("C:/Users/a/Downloads"),
            PathBuf::from("C:/Users/a/DocumentsOld"),
        ]);
        assert_eq!(
            got,
            [
                PathBuf::from("C:/Users/a/Documents"),
                PathBuf::from("C:/Users/a/Downloads"),
                PathBuf::from("C:/Users/a/DocumentsOld"),
            ]
        );
    }

    #[test]
    fn a_parent_given_later_still_wins() {
        let got = outermost(vec![
            PathBuf::from("D:/Photos/2020"),
            PathBuf::from("D:/Photos"),
        ]);
        assert_eq!(got, [PathBuf::from("D:/Photos")]);
    }

    #[test]
    fn xdg_user_dirs_are_read_with_fallbacks() {
        let home = Path::new("/home/nano");
        let text = "# written by xdg-user-dirs-update\n\
                    XDG_DESKTOP_DIR=\"$HOME/\"\n\
                    XDG_DOCUMENTS_DIR=\"$HOME/Tài liệu\"\n\
                    XDG_PICTURES_DIR=\"/home/nano/Ảnh\"\n\
                    XDG_MUSIC_DIR=\"/srv/music\"\n";
        let got = xdg_dirs(text, home);
        assert_eq!(
            got,
            [
                home.join("Tài liệu"),
                home.join("Downloads"),
                PathBuf::from("/home/nano/Ảnh"),
                home.join("Videos"),
            ]
        );
    }

    #[test]
    fn the_personal_threshold_is_below_the_default() {
        const { assert!(PERSONAL_MIN_SIZE < crate::DEFAULT_MIN_SIZE) };
        assert_eq!(PERSONAL_MIN_SIZE, 102_400);
    }

    #[test]
    fn personal_folders_exist_and_are_distinct() {
        let got = personal_folders();
        assert!(got.iter().all(|p| p.is_dir()));
        assert_eq!(outermost(got.clone()), got);
    }
}
