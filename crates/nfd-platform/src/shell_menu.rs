//! Explorer context menu "Analyze with NANOFreeDisk" and "Find duplicates here"
//! (PLAN §13 A7), registered per user under `HKCU\Software\Classes` — no
//! administrator rights, nothing machine-wide, and removing the keys
//! ([`unregister`]) leaves no trace. The entries start the app with the folder as
//! argument (`--dupes` first for the second one); a running instance takes it over.
//!
//! Keys written (each with a `command` sub-key `"<exe>" "%V"`, and the same under
//! [`DUPES_VERB`] with `"<exe>" --dupes "%V"`):
//! `Directory\shell\NANOFreeDisk` (right-click a folder),
//! `Directory\Background\shell\NANOFreeDisk` (right-click inside a folder) and
//! `Drive\shell\NANOFreeDisk` (right-click a drive).

use std::io;
use std::path::Path;

/// Verb key name under each `shell` key.
pub const VERB: &str = "NANOFreeDisk";
/// Verb key of "Find duplicates here".
pub const DUPES_VERB: &str = "NANOFreeDiskDupes";
/// Command-line flag of "Find duplicates here".
pub const DUPES_FLAG: &str = "--dupes";

/// `HKCU\Software\Classes\…` keys the menu lives under.
pub const PARENTS: [&str; 3] = [
    r"Software\Classes\Directory\shell",
    r"Software\Classes\Directory\Background\shell",
    r"Software\Classes\Drive\shell",
];

/// The command line Explorer runs: `"<exe>" "%V"`.
#[must_use]
pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" \"%V\"", exe.display())
}

/// The command line of "Find duplicates here": `"<exe>" --dupes "%V"`.
#[must_use]
pub fn dupes_command_line(exe: &Path) -> String {
    format!("\"{}\" {DUPES_FLAG} \"%V\"", exe.display())
}

/// Add (or update) both menu entries with the given captions, starting `exe`.
///
/// # Errors
/// The registry refuses the write (policy, roaming profile problems).
pub fn register(exe: &Path, caption: &str, dupes_caption: &str) -> io::Result<()> {
    imp::register(exe, caption, dupes_caption)
}

/// Remove every key [`register`] created. Missing keys are fine.
///
/// # Errors
/// The registry refuses the delete.
pub fn unregister() -> io::Result<()> {
    imp::unregister()
}

/// The command currently registered for folders, if any.
#[must_use]
pub fn registered_command() -> Option<String> {
    imp::registered_command()
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW,
    };

    use super::{DUPES_VERB, PARENTS, VERB, command_line, dupes_command_line};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    fn check(rc: WIN32_ERROR) -> io::Result<()> {
        if rc == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(rc as i32))
        }
    }

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the handle came from RegCreateKeyExW and is closed once.
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn create(path: &str) -> io::Result<Key> {
        let name = wide(path);
        let mut key: HKEY = ptr::null_mut();
        // SAFETY: valid NUL-terminated name, out-pointer to a local; no class/security.
        check(unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                name.as_ptr(),
                0,
                ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                ptr::null(),
                &mut key,
                ptr::null_mut(),
            )
        })?;
        Ok(Key(key))
    }

    fn set(key: &Key, value: Option<&str>, data: &str) -> io::Result<()> {
        let name = value.map(wide);
        let data = wide(data);
        // SAFETY: `data` is a NUL-terminated UTF-16 buffer of the given byte length.
        check(unsafe {
            RegSetValueExW(
                key.0,
                name.as_ref().map_or(ptr::null(), |n| n.as_ptr()),
                0,
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        })
    }

    pub(super) fn register(exe: &Path, caption: &str, dupes_caption: &str) -> io::Result<()> {
        let icon = format!("\"{}\",0", exe.display());
        for parent in PARENTS {
            for (name, text, line) in [
                (VERB, caption, command_line(exe)),
                (DUPES_VERB, dupes_caption, dupes_command_line(exe)),
            ] {
                let verb = create(&format!(r"{parent}\{name}"))?;
                set(&verb, None, text)?;
                set(&verb, Some("Icon"), &icon)?;
                let cmd = create(&format!(r"{parent}\{name}\command"))?;
                set(&cmd, None, &line)?;
            }
        }
        Ok(())
    }

    pub(super) fn unregister() -> io::Result<()> {
        for parent in PARENTS {
            for verb in [VERB, DUPES_VERB] {
                let name = wide(&format!(r"{parent}\{verb}"));
                // SAFETY: valid NUL-terminated sub-key name under HKCU.
                let rc = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, name.as_ptr()) };
                if rc != ERROR_FILE_NOT_FOUND {
                    check(rc)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn registered_command() -> Option<String> {
        let key = wide(&format!(r"{}\{VERB}\command", PARENTS[0]));
        let mut buf = vec![0u16; 1024];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: `buf` holds `bytes` bytes; default value (null name), string only.
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                ptr::null(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let len = (bytes as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..len.min(buf.len())]))
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::Path;

    pub(super) fn register(_exe: &Path, _caption: &str, _dupes: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Explorer menu is Windows-only",
        ))
    }

    pub(super) fn unregister() -> io::Result<()> {
        Ok(())
    }

    pub(super) fn registered_command() -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_exe_and_folder() {
        assert_eq!(
            command_line(Path::new(r"C:\Program Files\NANOFreeDisk\nanofreedisk.exe")),
            r#""C:\Program Files\NANOFreeDisk\nanofreedisk.exe" "%V""#
        );
        assert_eq!(
            dupes_command_line(Path::new(r"C:\Apps\nanofreedisk.exe")),
            r#""C:\Apps\nanofreedisk.exe" --dupes "%V""#
        );
    }
}
