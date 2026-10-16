//! Who owns a file or folder (PLAN §13 A9): the owner SID of its security
//! descriptor, turned into an account name (`DOMAIN\user`, `NT AUTHORITY\SYSTEM`,
//! `NT SERVICE\TrustedInstaller`…). Read-only; never changes permissions.

use std::path::Path;

/// Account name of the owner of `path`, or its SID string (`S-1-5-…`) when the
/// account cannot be resolved (deleted user, domain unreachable). `None` when the
/// security descriptor cannot be read or on platforms without owners yet.
#[must_use]
pub fn owner_name(path: &Path) -> Option<String> {
    imp::owner_name(path)
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        LookupAccountSidW, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SID_NAME_USE,
    };

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain([0]).collect()
    }

    fn from_wide(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    /// Frees the security descriptor returned by `GetNamedSecurityInfoW`.
    struct Descriptor(PSECURITY_DESCRIPTOR);

    impl Drop for Descriptor {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: allocated by GetNamedSecurityInfoW with LocalAlloc, freed once.
                unsafe { LocalFree(self.0) };
            }
        }
    }

    pub(super) fn owner_name(path: &Path) -> Option<String> {
        let name = wide(path);
        let mut owner: PSID = ptr::null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `name` is NUL-terminated; out-pointers are valid; only the owner is
        // requested, so the group/DACL/SACL pointers may be null.
        let rc = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut sd,
            )
        };
        let _guard = Descriptor(sd);
        if rc != ERROR_SUCCESS || owner.is_null() {
            return None;
        }
        let mut name_buf = [0u16; 256];
        let mut domain_buf = [0u16; 256];
        let mut name_len = name_buf.len() as u32;
        let mut domain_len = domain_buf.len() as u32;
        let mut use_: SID_NAME_USE = 0;
        // SAFETY: `owner` points into `sd`, alive until `_guard` drops; buffer lengths
        // are passed in characters.
        let ok = unsafe {
            LookupAccountSidW(
                ptr::null(),
                owner,
                name_buf.as_mut_ptr(),
                &mut name_len,
                domain_buf.as_mut_ptr(),
                &mut domain_len,
                &mut use_,
            )
        };
        if ok != 0 {
            let (domain, user) = (from_wide(&domain_buf), from_wide(&name_buf));
            return Some(if domain.is_empty() {
                user
            } else {
                format!("{domain}\\{user}")
            });
        }
        let mut text: *mut u16 = ptr::null_mut();
        // SAFETY: `owner` is a valid SID; on success `text` is a LocalAlloc'ed string.
        if unsafe { ConvertSidToStringSidW(owner, &mut text) } == 0 || text.is_null() {
            return None;
        }
        // SAFETY: `text` is NUL-terminated; read up to the terminator, then free it.
        let s = unsafe {
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text.cast());
            s
        };
        Some(s)
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    pub(super) fn owner_name(_path: &Path) -> Option<String> {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn owners_of_known_paths() {
        // a file we create belongs to us (or to Administrators when elevated)
        let dir = tempfile::tempdir().expect("tempdir");
        let f = dir.path().join("x.txt");
        std::fs::write(&f, b"x").expect("write");
        let me = owner_name(&f).expect("own file has an owner");
        assert!(me.contains('\\') || me.starts_with("S-1-"), "{me}");
        // System32 belongs to TrustedInstaller
        let sys = std::env::var_os("SystemRoot").map(|r| Path::new(&r).join("System32"));
        if let Some(sys) = sys {
            let owner = owner_name(&sys).expect("System32 owner");
            assert!(owner.to_lowercase().contains("trustedinstaller"), "{owner}");
        }
        assert_eq!(owner_name(&dir.path().join("missing")), None);
    }
}
