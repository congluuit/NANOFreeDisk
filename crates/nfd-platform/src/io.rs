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

/// macOS: the file is refused when it is *dataless* (iCloud Drive / File Provider
/// keeps its contents in the cloud — reading it would download it), and the
/// background threads are throttled (`IOPOL_THROTTLE`, utility QoS) and may not
/// download dataless files at all (`IOPOL_MATERIALIZE_DATALESS_FILES_OFF`: a read
/// fails instead).
#[cfg(target_os = "macos")]
mod imp {
    use std::fs::File;
    use std::io;
    use std::os::macos::fs::MetadataExt;
    use std::path::Path;

    use crate::fsinfo::macos_dataless;

    // <sys/resource.h>
    const IOPOL_TYPE_DISK: libc::c_int = 0;
    const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: libc::c_int = 3;
    const IOPOL_SCOPE_THREAD: libc::c_int = 1;
    const IOPOL_THROTTLE: libc::c_int = 3;
    const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: libc::c_int = 1;
    // <sys/qos.h>
    const QOS_CLASS_UTILITY: libc::c_uint = 0x11;

    unsafe extern "C" {
        fn setiopolicy_np(
            iotype: libc::c_int,
            scope: libc::c_int,
            policy: libc::c_int,
        ) -> libc::c_int;
        fn pthread_set_qos_class_self_np(
            qos_class: libc::c_uint,
            relative_priority: libc::c_int,
        ) -> libc::c_int;
    }

    pub(super) fn open_sequential(path: &Path) -> io::Result<File> {
        let file = File::open(path)?;
        if macos_dataless(file.metadata()?.st_flags()) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the file's contents are in the cloud (not downloaded)",
            ));
        }
        Ok(file)
    }

    pub(super) fn background_priority() {
        // SAFETY: plain syscalls on the calling thread; a failure only leaves it at
        // its normal priority / policy.
        unsafe {
            pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0);
            setiopolicy_np(IOPOL_TYPE_DISK, IOPOL_SCOPE_THREAD, IOPOL_THROTTLE);
            setiopolicy_np(
                IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
                IOPOL_SCOPE_THREAD,
                IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
            );
        }
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
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
