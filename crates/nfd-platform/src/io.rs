//! Reading file contents politely (Phase 5, Duplicates).
//!
//! [`open_sequential`] opens a file for reading with `FILE_FLAG_SEQUENTIAL_SCAN` (the
//! cache manager reads ahead and drops pages behind) and shares read, write and
//! delete access, so the app reading a file never stops another program from saving,
//! renaming or deleting it. [`background_priority`] puts the calling thread in
//! *background mode*: low CPU **and** low I/O priority, so hashing 100 GB does not
//! make the rest of the PC sluggish.

use std::fs::File;
use std::io;
use std::path::Path;

/// Open `path` read-only for one sequential pass, sharing everything.
///
/// Callers must check first that the file is not a cloud placeholder: opening one
/// for reading makes the sync engine download it.
///
/// # Errors
/// The OS error of `CreateFileW` / `open`.
pub fn open_sequential(path: &Path) -> io::Result<File> {
    imp::open_sequential(path)
}

/// Lower the calling thread's CPU and I/O priority until it ends (Windows background
/// mode). Does nothing elsewhere.
pub fn background_priority() {
    imp::background_priority();
}

#[cfg(windows)]
mod imp {
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_SEQUENTIAL_SCAN, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
    };

    pub(super) fn open_sequential(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_SEQUENTIAL_SCAN)
            .open(path)
    }

    pub(super) fn background_priority() {
        // SAFETY: pseudo-handle of the current thread; failure only means the thread
        // keeps its normal priority.
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::fs::File;
    use std::io;
    use std::path::Path;

    pub(super) fn open_sequential(path: &Path) -> io::Result<File> {
        File::open(path)
    }

    pub(super) fn background_priority() {}
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn reads_and_lets_others_delete() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join("a.bin");
        std::fs::write(&p, b"hello").expect("write");
        background_priority();
        let mut f = open_sequential(&p).expect("open");
        // another program may still rename the file while we hold it
        std::fs::rename(&p, dir.path().join("b.bin")).expect("rename while open");
        let mut s = String::new();
        f.read_to_string(&mut s).expect("read");
        assert_eq!(s, "hello");
    }
}
