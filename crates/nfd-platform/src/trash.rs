//! Move files to the Recycle Bin (PLAN §3.4 `DeleteMode::RecycleBin`).
//!
//! Windows uses the shell (`SHFileOperationW` with `FOF_ALLOWUNDO`), so the item can be
//! restored from Explorer exactly as if the user had pressed Delete. No progress or
//! confirmation UI is shown — the application confirms beforehand — **except** the
//! shell's own "permanently delete?" prompt (`FOF_WANTNUKEWARNING`) when an item cannot
//! be recycled (too large for the bin, removable drive, network share): nothing is
//! ever destroyed silently.
//!
//! macOS uses `NSFileManager trashItemAtURL:resultingItemURL:error:` — the same
//! move as Finder's "Move to Trash" (Put Back works), to the Trash of the item's
//! volume. The resulting path is returned by [`recycle_with_location`] so the app
//! can put the item back itself ([`restore_trashed`]); the macOS Trash keeps no
//! record of where an item came from that other programs can read.
//!
//! Callers must run the safety checks first (`nfd_core::safety`); this module only
//! performs the move.

use std::io;
use std::path::{Path, PathBuf};

/// Outcome of a successful [`recycle`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recycled {
    /// The item is in the Recycle Bin.
    Done,
    /// The user declined the shell's "permanently delete?" prompt; nothing changed.
    Declined,
}

/// Move one file or directory (with its contents) to the Recycle Bin.
///
/// `owner` is the native window handle the shell's prompt is attached to (`0` for
/// none). Blocks until the shell is done, so call it off the UI thread.
///
/// # Errors
/// The shell's failure, or [`io::ErrorKind::Unsupported`] on platforms without a
/// recycle bin implementation yet.
pub fn recycle(path: &Path, owner: isize) -> io::Result<Recycled> {
    recycle_with_location(path, owner).map(|(r, _)| r)
}

/// [`recycle`], plus where the item now is when the platform says so (macOS: its
/// path inside the Trash; Windows: `None`, the bin's `$I` records are read
/// instead, [`find_recycled`]).
///
/// # Errors
/// As [`recycle`].
pub fn recycle_with_location(path: &Path, owner: isize) -> io::Result<(Recycled, Option<PathBuf>)> {
    imp::recycle(path, owner)
}

/// Put an item moved to the Trash at `trashed` back at `original` (missing parent
/// folders are created again).
///
/// # Errors
/// Something already exists at the original path, or the move fails.
pub fn restore_trashed(trashed: &Path, original: &Path) -> io::Result<()> {
    if original.symlink_metadata().is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "something with the same name is already there",
        ));
    }
    if let Some(parent) = original.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(trashed, original)
}

/// An item found in the Recycle Bin (Windows `$I…` / `$R…` pair).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecycledItem {
    /// Where the item was deleted from.
    pub original: PathBuf,
    /// When it was deleted (Unix seconds).
    pub deleted_at: i64,
    /// Size recorded by the shell (bytes).
    pub size: u64,
    /// The `$I…` metadata file.
    pub info_file: PathBuf,
    /// The `$R…` data (file or folder).
    pub data: PathBuf,
}

/// Parse a `$I…` metadata file (format 1: Vista–8.1, format 2: Windows 10+).
/// Returns `(original path, deleted at (Unix s), size)`.
#[must_use]
pub fn parse_info(bytes: &[u8]) -> Option<(PathBuf, i64, u64)> {
    let u64_at = |at: usize| -> Option<u64> {
        Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
    };
    let utf16 = |raw: &[u8]| -> Vec<u16> {
        raw.chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect()
    };
    let version = u64_at(0)?;
    let size = u64_at(8)?;
    let filetime = u64_at(16)?;
    let units = match version {
        1 => utf16(bytes.get(24..24 + 520)?),
        2 => {
            let len = u32::from_le_bytes(bytes.get(24..28)?.try_into().ok()?) as usize;
            utf16(bytes.get(28..28 + len.checked_mul(2)?)?)
        }
        _ => return None,
    };
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    let path = String::from_utf16(&units[..end]).ok()?;
    if path.is_empty() {
        return None;
    }
    // FILETIME: 100 ns since 1601-01-01.
    let unix = i64::try_from(filetime / 10_000_000).ok()? - 11_644_473_600;
    Some((PathBuf::from(path), unix, size))
}

/// Items in the Recycle Bin of the volume of `original` that were deleted from
/// exactly `original` (case-insensitive), newest first. Only the current user's bin
/// is readable; other users' folders are skipped. The shell records the long form of
/// the path, so an 8.3 short name in `original` (`C:\Users\RUNNER~1\...`) also
/// matches.
///
/// # Errors
/// The volume's Recycle Bin cannot be listed.
pub fn find_recycled(original: &Path) -> io::Result<Vec<RecycledItem>> {
    if cfg!(not(windows)) {
        // no `$Recycle.Bin` here: trashed items are found by the path recorded when
        // they were moved ([`recycle_with_location`])
        return Ok(Vec::new());
    }
    let Some(std::path::Component::Prefix(prefix)) = original.components().next() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be absolute",
        ));
    };
    let mut bin = PathBuf::from(prefix.as_os_str());
    bin.push(std::path::MAIN_SEPARATOR_STR);
    bin.push("$Recycle.Bin");
    let mut want = vec![original.to_string_lossy().to_lowercase()];
    if let Some(long) = long_form(original) {
        want.push(long.to_string_lossy().to_lowercase());
    }
    let mut out = Vec::new();
    for sid in std::fs::read_dir(&bin)?.flatten() {
        let Ok(files) = std::fs::read_dir(sid.path()) else {
            continue;
        };
        for f in files.flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            let Some(suffix) = name.strip_prefix("$I") else {
                continue;
            };
            let Ok(bytes) = std::fs::read(f.path()) else {
                continue;
            };
            let Some((path, deleted_at, size)) = parse_info(&bytes) else {
                continue;
            };
            if !want.contains(&path.to_string_lossy().to_lowercase()) {
                continue;
            }
            let data = sid.path().join(format!("$R{suffix}"));
            if data.symlink_metadata().is_ok() {
                out.push(RecycledItem {
                    original: path,
                    deleted_at,
                    size,
                    info_file: f.path(),
                    data,
                });
            }
        }
    }
    out.sort_by_key(|i| std::cmp::Reverse(i.deleted_at));
    Ok(out)
}

/// `original` with its (existing) parent folder resolved to the full long-name form,
/// without the `\\?\` prefix `canonicalize` adds; `None` when that cannot be done.
fn long_form(original: &Path) -> Option<PathBuf> {
    let parent = std::fs::canonicalize(original.parent()?).ok()?;
    let parent = parent.to_str()?.strip_prefix(r"\\?\")?;
    if parent.starts_with(r"UNC\") {
        return None;
    }
    Some(Path::new(parent).join(original.file_name()?))
}

/// Put a recycled item back where it was deleted from (like Explorer's "Restore").
/// Missing parent folders are created again.
///
/// # Errors
/// Something already exists at the original path, or the move fails.
pub fn restore_recycled(item: &RecycledItem) -> io::Result<()> {
    if item.original.symlink_metadata().is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "something with the same name is already there",
        ));
    }
    if let Some(parent) = item.original.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&item.data, &item.original)?;
    let _ = std::fs::remove_file(&item.info_file);
    Ok(())
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::ptr;

    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
        FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW, SHFileOperationW,
    };

    use super::Recycled;

    pub(super) fn recycle(path: &Path, owner: isize) -> io::Result<(Recycled, Option<PathBuf>)> {
        recycle_shell(path, owner).map(|r| (r, None))
    }

    fn recycle_shell(path: &Path, owner: isize) -> io::Result<Recycled> {
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path must be absolute",
            ));
        }
        // The shell does not understand verbatim (`\\?\`) paths.
        let text = path.as_os_str().to_string_lossy();
        if text.starts_with(r"\\?\") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "verbatim paths are not supported by the shell",
            ));
        }
        // pFrom is a list of NUL-terminated paths ending with an extra NUL.
        let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
        if from.contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"));
        }
        from.extend([0, 0]);
        let mut op = SHFILEOPSTRUCTW {
            hwnd: owner as _,
            wFunc: FO_DELETE,
            pFrom: from.as_ptr(),
            pTo: ptr::null(),
            fFlags: (FOF_ALLOWUNDO
                | FOF_NOCONFIRMATION
                | FOF_SILENT
                | FOF_NOERRORUI
                | FOF_WANTNUKEWARNING) as u16,
            fAnyOperationsAborted: 0,
            hNameMappings: ptr::null_mut(),
            lpszProgressTitle: ptr::null(),
        };
        // SAFETY: `op` is fully initialised; `from` is double-NUL-terminated and
        // outlives the call; `pTo` is unused for FO_DELETE.
        let rc = unsafe { SHFileOperationW(&mut op) };
        if op.fAnyOperationsAborted != 0 {
            return Ok(Recycled::Declined);
        }
        if rc != 0 {
            // SHFileOperation returns legacy DE_* codes, mostly equal to Win32 errors.
            return Err(io::Error::from_raw_os_error(rc));
        }
        match path.symlink_metadata() {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Recycled::Done),
            Ok(_) => Err(io::Error::other("the item is still there after recycling")),
            Err(e) => Err(e),
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::io;
    use std::path::{Path, PathBuf};

    use objc2_foundation::{NSFileManager, NSString, NSURL};

    use super::Recycled;

    pub(super) fn recycle(path: &Path, _owner: isize) -> io::Result<(Recycled, Option<PathBuf>)> {
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path must be absolute",
            ));
        }
        // never follows a link: the item itself goes, as in Finder
        path.symlink_metadata()?;
        let text = path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path is not UTF-8"))?;
        let trashed = objc2::rc::autoreleasepool(|_| {
            let url = NSURL::fileURLWithPath(&NSString::from_str(text));
            let mut resulting = None;
            NSFileManager::defaultManager()
                .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
                .map_err(|e| io::Error::other(e.localizedDescription().to_string()))?;
            Ok::<_, io::Error>(
                resulting
                    .and_then(|u| u.path())
                    .map(|p| PathBuf::from(p.to_string())),
            )
        })?;
        match path.symlink_metadata() {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok((Recycled::Done, trashed)),
            Ok(_) => Err(io::Error::other("the item is still there after trashing")),
            Err(e) => Err(e),
        }
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
mod imp {
    use std::io;
    use std::path::{Path, PathBuf};

    use super::Recycled;

    pub(super) fn recycle(_path: &Path, _owner: isize) -> io::Result<(Recycled, Option<PathBuf>)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "the trash is only implemented on Windows and macOS",
        ))
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;

    fn v2(path: &str, size: u64, filetime: u64) -> Vec<u8> {
        let units: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let mut b = Vec::new();
        b.extend(2u64.to_le_bytes());
        b.extend(size.to_le_bytes());
        b.extend(filetime.to_le_bytes());
        b.extend((units.len() as u32).to_le_bytes());
        for u in units {
            b.extend(u.to_le_bytes());
        }
        b
    }

    #[test]
    fn parses_info_v2_and_v1() {
        // 2020-01-01T00:00:00Z
        let ft = (1_577_836_800u64 + 11_644_473_600) * 10_000_000;
        let (p, at, size) = parse_info(&v2(r"C:\x\y.txt", 42, ft)).expect("v2");
        assert_eq!(p, PathBuf::from(r"C:\x\y.txt"));
        assert_eq!((at, size), (1_577_836_800, 42));
        let mut v1 = Vec::new();
        v1.extend(1u64.to_le_bytes());
        v1.extend(7u64.to_le_bytes());
        v1.extend(ft.to_le_bytes());
        let mut name: Vec<u8> = r"D:\a".encode_utf16().flat_map(u16::to_le_bytes).collect();
        name.resize(520, 0);
        v1.extend(name);
        let (p, _, size) = parse_info(&v1).expect("v1");
        assert_eq!((p, size), (PathBuf::from(r"D:\a"), 7));
        assert!(parse_info(&[0; 10]).is_none());
        assert!(parse_info(&v2("", 1, ft)).is_none());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;

    #[test]
    fn trashed_file_comes_back() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("nfd-trash-test.txt");
        std::fs::write(&file, b"restore me").expect("write");
        let (r, at) = recycle_with_location(&file, 0).expect("trash");
        assert_eq!(r, Recycled::Done);
        assert!(!file.exists());
        let at = at.expect("the Trash location");
        assert!(at.exists(), "{}", at.display());
        restore_trashed(&at, &file).expect("put back");
        assert_eq!(std::fs::read(&file).expect("read"), b"restore me");
        assert!(find_recycled(&file).expect("no bin").is_empty());
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn recycled_file_can_be_found_and_restored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("nfd-restore-test.txt");
        std::fs::write(&file, b"restore me").expect("write");
        assert_eq!(recycle(&file, 0).expect("recycle"), Recycled::Done);
        let found = find_recycled(&file).expect("bin");
        let item = found.first().expect("found in the bin");
        assert_eq!(item.size, 10);
        restore_recycled(item).expect("restore");
        assert_eq!(std::fs::read(&file).expect("read"), b"restore me");
        assert!(find_recycled(&file).expect("bin").is_empty());
    }

    #[test]
    fn recycles_a_temp_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("nfd-recycle-test.txt");
        std::fs::write(&file, b"hello").expect("write");
        assert_eq!(recycle(&file, 0).expect("recycle"), Recycled::Done);
        assert!(!file.exists());
    }

    #[test]
    fn rejects_relative_and_missing_paths() {
        assert!(recycle(Path::new("relative.txt"), 0).is_err());
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(recycle(&dir.path().join("missing"), 0).is_err());
    }
}
