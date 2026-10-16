//! Folders only the current user (plus SYSTEM and Administrators) can open — used for
//! the quarantine on drives other than the system drive (`<volume>\.nfd-quarantine`),
//! so cached files of one user are not listed to another (PHASE3_CLEAN §2.3).
//!
//! On Windows the folder is created with a protected DACL (`D:P`, no inheritance from
//! the drive root) granting full control to `OWNER RIGHTS`, `SYSTEM` and
//! `Administrators`, and optionally marked hidden. Elsewhere it is created with mode
//! `0700`.

use std::io;
use std::path::Path;

/// Create `path` (its parent must exist) as a private folder. An existing folder is
/// left as it is.
///
/// # Errors
/// The folder cannot be created.
pub fn create(path: &Path, hidden: bool) -> io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    imp::create(path, hidden)
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateDirectoryW, FILE_ATTRIBUTE_HIDDEN, SetFileAttributesW,
    };

    /// Owner rights, SYSTEM, Administrators: full control, inherited by children.
    const SDDL: &str = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain([0]).collect()
    }

    pub(super) fn create(path: &Path, hidden: bool) -> io::Result<()> {
        let sddl = wide(SDDL.as_ref());
        let mut sd: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `sddl` is NUL-terminated; `sd` receives a LocalAlloc'd descriptor
        // that is freed below.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let attrs = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let name = wide(path.as_os_str());
        // SAFETY: `name` is NUL-terminated, `attrs` points at a valid descriptor.
        let created = unsafe { CreateDirectoryW(name.as_ptr(), &attrs) };
        let err = io::Error::last_os_error();
        // SAFETY: `sd` came from ConvertStringSecurityDescriptor…, freed once.
        unsafe { LocalFree(sd) };
        if created == 0 {
            return Err(err);
        }
        if hidden {
            // SAFETY: as above.
            unsafe { SetFileAttributesW(name.as_ptr(), FILE_ATTRIBUTE_HIDDEN) };
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::Path;

    pub(super) fn create(path: &Path, _hidden: bool) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new().mode(0o700).create(path)
        }
        #[cfg(not(unix))]
        {
            std::fs::create_dir(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_once_and_is_usable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let q = dir.path().join(".nfd-quarantine");
        create(&q, true).expect("create");
        create(&q, true).expect("idempotent");
        std::fs::write(q.join("x"), b"1").expect("owner can write");
        assert_eq!(std::fs::read(q.join("x")).expect("read"), b"1");
    }
}
