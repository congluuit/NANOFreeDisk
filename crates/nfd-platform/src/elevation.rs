//! What a one-shot elevated process (`--elevated-clean`) does to protect itself
//! before it touches anything.
//!
//! * [`machine_folder`] resolves the machine-wide rule-pack variables from Windows
//!   itself (`SHGetKnownFolderPath`, which reads them from `HKEY_LOCAL_MACHINE`),
//!   never from the environment: an elevated process started through UAC gets the
//!   user's environment, and a variable such as `SystemRoot` set in
//!   `HKEY_CURRENT_USER\Environment` is the classic way to point an elevated
//!   program somewhere else (the "SilentCleanup" bypass).
//! * [`enforce_redirection_trust`] turns on Windows' *Redirection Guard*
//!   (`ProcessRedirectionTrustPolicy`): the process no longer follows junctions
//!   created by a non-administrator, which closes the race where a folder is swapped
//!   for a junction between the last check and the delete (the cause of several
//!   "elevated cleaner deletes the wrong files" bugs, CVE-2025-21420 among them).
//!   Windows versions without it report `false` and the checks of the executor still
//!   apply.

use std::path::PathBuf;

/// The machine-wide folder behind a rule-pack variable (`SYSTEMROOT`,
/// `SYSTEMDRIVE`, `PROGRAMDATA`, `PROGRAMFILES`, `PROGRAMFILES_X86`) as Windows
/// knows it; `None` for any other variable or on other platforms.
#[must_use]
pub fn machine_folder(var: &str) -> Option<PathBuf> {
    imp::machine_folder(var)
}

/// Refuse to follow junctions that a non-administrator created, for the rest of
/// this process's life. `true` when Windows accepted it.
#[must_use]
pub fn enforce_redirection_trust() -> bool {
    imp::enforce_redirection_trust()
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::System::SystemServices::PROCESS_MITIGATION_REDIRECTION_TRUST_POLICY;
    use windows_sys::Win32::System::Threading::{
        ProcessRedirectionTrustPolicy, SetProcessMitigationPolicy,
    };
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_Windows,
        SHGetKnownFolderPath,
    };
    use windows_sys::core::{GUID, PWSTR};

    fn known(id: &GUID) -> Option<PathBuf> {
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
                Some(PathBuf::from(OsString::from_wide(
                    std::slice::from_raw_parts(p, n),
                )))
            } else {
                None
            };
            CoTaskMemFree(p.cast());
            out
        }
    }

    pub(super) fn machine_folder(var: &str) -> Option<PathBuf> {
        match var {
            "SYSTEMROOT" => known(&FOLDERID_Windows),
            "SYSTEMDRIVE" => {
                // `C:` of the Windows folder
                let win = known(&FOLDERID_Windows)?;
                let text = win.to_string_lossy();
                let drive = text.get(..2).filter(|d| d.ends_with(':'))?;
                Some(PathBuf::from(drive))
            }
            "PROGRAMDATA" => known(&FOLDERID_ProgramData),
            "PROGRAMFILES" => known(&FOLDERID_ProgramFiles),
            "PROGRAMFILES_X86" => known(&FOLDERID_ProgramFilesX86),
            _ => None,
        }
    }

    pub(super) fn enforce_redirection_trust() -> bool {
        let mut policy = PROCESS_MITIGATION_REDIRECTION_TRUST_POLICY::default();
        // bit 0: EnforceRedirectionTrust
        policy.Anonymous.Flags = 1;
        // SAFETY: the buffer is the policy structure of the size given; the call
        // only reads it.
        unsafe {
            SetProcessMitigationPolicy(
                ProcessRedirectionTrustPolicy,
                (&raw const policy).cast(),
                size_of::<PROCESS_MITIGATION_REDIRECTION_TRUST_POLICY>(),
            ) != 0
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::PathBuf;

    pub(super) fn machine_folder(_: &str) -> Option<PathBuf> {
        None
    }

    pub(super) fn enforce_redirection_trust() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_machine_variables_resolve() {
        for var in [
            "HOME",
            "TEMP",
            "APPDATA",
            "LOCALAPPDATA",
            "USERPROFILE",
            "x",
        ] {
            assert_eq!(machine_folder(var), None, "{var}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn machine_folders_come_from_windows() {
        let win = machine_folder("SYSTEMROOT").expect("Windows folder");
        assert!(win.is_absolute() && win.is_dir(), "{}", win.display());
        let drive = machine_folder("SYSTEMDRIVE").expect("drive");
        assert!(win.starts_with(format!("{}\\", drive.display())));
        assert!(machine_folder("PROGRAMDATA").is_some_and(|p| p.is_dir()));
        assert!(machine_folder("PROGRAMFILES").is_some_and(|p| p.is_dir()));
    }
}
