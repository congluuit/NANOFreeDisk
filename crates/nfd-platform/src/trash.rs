//! Move files to the Recycle Bin (PLAN §3.4 `DeleteMode::RecycleBin`).
//!
//! Windows uses the shell (`SHFileOperationW` with `FOF_ALLOWUNDO`), so the item can be
//! restored from Explorer exactly as if the user had pressed Delete. No progress or
//! confirmation UI is shown — the application confirms beforehand — **except** the
//! shell's own "permanently delete?" prompt (`FOF_WANTNUKEWARNING`) when an item cannot
//! be recycled (too large for the bin, removable drive, network share): nothing is
//! ever destroyed silently.
//!
//! Callers must run the safety checks first (`nfd_core::safety`); this module only
//! performs the move.

use std::io;
use std::path::Path;

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
    imp::recycle(path, owner)
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
        FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW, SHFileOperationW,
    };

    use super::Recycled;

    pub(super) fn recycle(path: &Path, owner: isize) -> io::Result<Recycled> {
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

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::Path;

    use super::Recycled;

    pub(super) fn recycle(_path: &Path, _owner: isize) -> io::Result<Recycled> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "the Recycle Bin is only implemented on Windows so far",
        ))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

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
