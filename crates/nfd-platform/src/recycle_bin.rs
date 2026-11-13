//! The Recycle Bin as a whole: how much it holds per drive, and emptying it.
//!
//! PLAN §5.1 lets nothing touch `$Recycle.Bin` except the shell's own Empty Recycle
//! Bin API, so this module never deletes a file itself: the size and item count come
//! from `SHQueryRecycleBinW`, emptying is `SHEmptyRecycleBinW` with no shell
//! confirmation, progress window or sound (the application asks first, and says
//! plainly that it cannot be undone). The same two calls BleachBit and Explorer's
//! "Empty Recycle Bin" use.
//!
//! [`recorded_items`] reads the bin's `$I…` records of one drive (the format
//! [`crate::trash::parse_info`] decodes) so the undo journal can learn which of its
//! Recycle Bin items are gone.
//!
//! Windows only: elsewhere every call is [`io::ErrorKind::Unsupported`] (the macOS
//! Trash is handled by [`crate::trash`]).

use std::io;
use std::path::{Path, PathBuf};

/// What the Recycle Bin holds, for one drive or all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinInfo {
    /// The drive (`C:\`), or `None` for all drives together.
    pub root: Option<PathBuf>,
    /// Bytes the deleted items take.
    pub bytes: u64,
    /// Deleted items (a folder counts once).
    pub items: u64,
}

/// One item the bin's records of a drive name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedItem {
    /// Where it was deleted from.
    pub original: PathBuf,
    /// When (Unix seconds).
    pub deleted_at: i64,
}

/// What the Recycle Bin of drive `root` (`C:\`) holds, or of every drive with
/// `None`. Only the current user's items count, as in Explorer.
///
/// # Errors
/// The drive has no Recycle Bin (network or optical drives, some removable
/// ones), or the platform has none ([`io::ErrorKind::Unsupported`]).
pub fn query(root: Option<&Path>) -> io::Result<BinInfo> {
    imp::query(root)
}

/// Empty the Recycle Bin of drive `root`, or of every drive with `None`. Permanent:
/// nothing in it can be restored afterwards. No shell prompt, progress or sound.
///
/// # Errors
/// The shell's failure (an item in use, no rights), or no Recycle Bin here.
pub fn empty(root: Option<&Path>) -> io::Result<()> {
    imp::empty(root)
}

/// The `$I…` records of the bin of drive `root`: the current user's folder and
/// any other readable one. `NotFound` when the drive has no `$Recycle.Bin`.
///
/// # Errors
/// The `$Recycle.Bin` folder cannot be listed.
pub fn recorded_items(root: &Path) -> io::Result<Vec<RecordedItem>> {
    let bin = root.join("$Recycle.Bin");
    let mut out = Vec::new();
    for sid in std::fs::read_dir(&bin)?.flatten() {
        let Ok(files) = std::fs::read_dir(sid.path()) else {
            continue; // another user's folder
        };
        for f in files.flatten() {
            if !f.file_name().to_string_lossy().starts_with("$I") {
                continue;
            }
            let Ok(bytes) = std::fs::read(f.path()) else {
                continue;
            };
            if let Some((original, deleted_at, _)) = crate::trash::parse_info(&bytes) {
                out.push(RecordedItem {
                    original,
                    deleted_at,
                });
            }
        }
    }
    Ok(out)
}

/// The drive root of an absolute Windows path (`C:\x\y` gives `C:\`), the letter
/// in upper case; `None` for relative, UNC or verbatim paths.
#[must_use]
pub fn drive_of(path: &str) -> Option<String> {
    let b = path.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        .then(|| format!("{}:\\", char::from(b[0]).to_ascii_uppercase()))
}

/// Whether `records` (of the item's drive) still hold an item deleted from
/// `original` by an operation that started at `started_at`. Compared by file name
/// (case-insensitive) and time rather than the full path, so a short 8.3 parent
/// (`C:\PROGRA~1\…`) recorded one way and journaled another still matches: when
/// unsure, the item counts as still there.
#[must_use]
pub fn still_recorded(records: &[RecordedItem], original: &Path, started_at: i64) -> bool {
    let Some(name) = original
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
    else {
        return true;
    };
    records.iter().any(|r| {
        r.deleted_at >= started_at - 60
            && r.original
                .file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase() == name)
    })
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::UI::Shell::{
        SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW, SHQUERYRBINFO,
        SHQueryRecycleBinW,
    };

    use super::BinInfo;

    fn wide(p: &Path) -> io::Result<Vec<u16>> {
        let w: Vec<u16> = p.as_os_str().encode_wide().collect();
        if w.contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"));
        }
        Ok(w.into_iter().chain(std::iter::once(0)).collect())
    }

    fn hresult(hr: i32) -> io::Error {
        // HRESULT_FROM_WIN32: the Win32 code is the low word
        if (hr as u32) & 0xFFFF_0000 == 0x8007_0000 {
            io::Error::from_raw_os_error(hr & 0xFFFF)
        } else {
            io::Error::other(format!("shell error 0x{:08x}", hr as u32))
        }
    }

    pub(super) fn query(root: Option<&Path>) -> io::Result<BinInfo> {
        let path = root.map(wide).transpose()?;
        let mut info = SHQUERYRBINFO {
            cbSize: size_of::<SHQUERYRBINFO>() as u32,
            i64Size: 0,
            i64NumItems: 0,
        };
        // SAFETY: a NUL-terminated root (or null for every drive) and a sized struct.
        let hr = unsafe {
            SHQueryRecycleBinW(path.as_ref().map_or(ptr::null(), |p| p.as_ptr()), &mut info)
        };
        if hr < 0 {
            return Err(hresult(hr));
        }
        Ok(BinInfo {
            root: root.map(Path::to_path_buf),
            bytes: u64::try_from(info.i64Size).unwrap_or(0),
            items: u64::try_from(info.i64NumItems).unwrap_or(0),
        })
    }

    pub(super) fn empty(root: Option<&Path>) -> io::Result<()> {
        let path = root.map(wide).transpose()?;
        // SAFETY: as above; no owner window, the shell shows nothing.
        let hr = unsafe {
            SHEmptyRecycleBinW(
                ptr::null_mut(),
                path.as_ref().map_or(ptr::null(), |p| p.as_ptr()),
                SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
            )
        };
        if hr >= 0 {
            return Ok(());
        }
        // an already empty bin answers E_UNEXPECTED on some Windows versions
        match query(root) {
            Ok(info) if info.items == 0 => Ok(()),
            _ => Err(hresult(hr)),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::Path;

    use super::BinInfo;

    fn none() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "the Recycle Bin card is Windows-only",
        )
    }

    pub(super) fn query(_: Option<&Path>) -> io::Result<BinInfo> {
        Err(none())
    }

    pub(super) fn empty(_: Option<&Path>) -> io::Result<()> {
        Err(none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drives_of_paths() {
        assert_eq!(drive_of(r"c:\Users\x.txt").as_deref(), Some(r"C:\"));
        assert_eq!(drive_of("D:/a").as_deref(), Some(r"D:\"));
        assert_eq!(drive_of(r"\\server\share\a"), None);
        assert_eq!(drive_of(r"\\?\C:\a"), None);
        assert_eq!(drive_of("relative"), None);
        assert_eq!(drive_of("C:"), None);
    }

    #[test]
    fn records_match_by_name_and_time() {
        let records = vec![RecordedItem {
            original: PathBuf::from(r"C:\Program Files\Acme\Log.TXT"),
            deleted_at: 1_000,
        }];
        // the short form of the parent still matches
        assert!(still_recorded(
            &records,
            Path::new(r"C:\PROGRA~1\Acme\log.txt"),
            1_000
        ));
        // a minute of clock slack
        assert!(still_recorded(&records, Path::new(r"C:\x\log.txt"), 1_050));
        // deleted before this operation: an older copy, not ours
        assert!(!still_recorded(&records, Path::new(r"C:\x\log.txt"), 2_000));
        assert!(!still_recorded(&records, Path::new(r"C:\x\other.txt"), 900));
        assert!(!still_recorded(&[], Path::new(r"C:\x\log.txt"), 900));
    }

    #[cfg(windows)]
    #[test]
    fn the_whole_bin_can_be_queried() {
        // reading only: emptying the real bin in a test would destroy user data
        let all = query(None).expect("query");
        assert!(all.root.is_none());
        let c = query(Some(Path::new(r"C:\"))).expect("C:");
        assert!(c.items <= all.items);
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_elsewhere() {
        assert_eq!(
            query(None).map_err(|e| e.kind()).err(),
            Some(io::ErrorKind::Unsupported)
        );
    }
}
