//! Writing one registry value, for the startup manager's enable / disable
//! (`docs/STARTUP.md`): Task Manager's `StartupApproved` flags are values the
//! app sets, never deletes. Kept apart from [`crate::registry`], whose writes are
//! all deletions behind a `.reg` backup.
//!
//! Always the 64-bit view, like [`crate::registry`].

use std::io;

use crate::registry::{Hive, Value};

/// Set value `name` of `hive\path` (the key is created when missing).
///
/// # Errors
/// No rights (`PermissionDenied`, an `HKEY_LOCAL_MACHINE` key without elevation),
/// or the registry refused the write.
pub fn set_value(hive: Hive, path: &str, name: &str, value: &Value) -> io::Result<()> {
    imp::set_value(hive, path, name, value)
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE,
        KEY_WOW64_64KEY, REG_OPTION_NON_VOLATILE, RegCloseKey, RegCreateKeyExW, RegSetValueExW,
    };

    use super::{Hive, Value};

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub(super) fn set_value(hive: Hive, path: &str, name: &str, value: &Value) -> io::Result<()> {
        let root: HKEY = match hive {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        };
        let p = wide(path.trim_matches('\\'));
        let mut key: HKEY = ptr::null_mut();
        // SAFETY: NUL-terminated subkey, valid out pointers; the handle is closed below.
        let rc = unsafe {
            RegCreateKeyExW(
                root,
                p.as_ptr(),
                0,
                ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_QUERY_VALUE | KEY_WOW64_64KEY,
                ptr::null(),
                &mut key,
                ptr::null_mut(),
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(rc as i32));
        }
        let n = wide(name);
        let data = value.raw();
        // SAFETY: open key, NUL-terminated name, `data` holds `len` bytes.
        let rc = unsafe {
            RegSetValueExW(
                key,
                n.as_ptr(),
                0,
                value.kind(),
                data.as_ptr(),
                data.len() as u32,
            )
        };
        // SAFETY: opened above, closed once.
        unsafe {
            RegCloseKey(key);
        }
        if rc == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(rc as i32))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;

    use super::{Hive, Value};

    pub(super) fn set_value(_: Hive, _: &str, _: &str, _: &Value) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no registry on this platform",
        ))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn sets_and_reads_back_a_user_value() {
        // its own root: the registry tests remove `Software\NANOFreeDiskTest`
        let key = format!(r"Software\NANOFreeDiskWriteTest\w-{}", std::process::id());
        let v = Value::Binary(vec![3, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
        set_value(Hive::CurrentUser, &key, "Tool.lnk", &v).expect("set");
        assert_eq!(
            crate::registry::value(Hive::CurrentUser, &key, "Tool.lnk").expect("read"),
            Some(v)
        );
        crate::registry::delete_tree(Hive::CurrentUser, r"Software\NANOFreeDiskWriteTest")
            .expect("cleanup");
    }
}
