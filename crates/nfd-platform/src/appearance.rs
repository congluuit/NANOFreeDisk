//! Desktop appearance settings the UI follows in its *System* theme (PLAN §11).

/// The user's accent color as `#rrggbb`, if the platform exposes one.
///
/// Windows: `HKCU\Software\Microsoft\Windows\DWM\AccentColor` (ABGR `DWORD`).
#[must_use]
pub fn accent_color() -> Option<String> {
    imp::accent_color()
}

#[cfg(windows)]
mod imp {
    use std::ptr;

    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

    pub(super) fn accent_color() -> Option<String> {
        let key: Vec<u16> = "Software\\Microsoft\\Windows\\DWM\0"
            .encode_utf16()
            .collect();
        let value: Vec<u16> = "AccentColor\0".encode_utf16().collect();
        let mut data = 0u32;
        let mut size = 4u32;
        // SAFETY: both names are NUL-terminated; `data` is a writable DWORD whose size
        // is passed in `size`.
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                ptr::null_mut(),
                (&raw mut data).cast(),
                &mut size,
            )
        };
        (rc == 0).then(|| {
            let [r, g, b, _a] = data.to_le_bytes();
            format!("#{r:02x}{g:02x}{b:02x}")
        })
    }
}

#[cfg(not(windows))]
mod imp {
    pub(super) fn accent_color() -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn accent_is_a_hex_color_when_present() {
        if let Some(c) = super::accent_color() {
            assert_eq!(c.len(), 7);
            assert!(c.starts_with('#'));
            assert!(c[1..].chars().all(|ch| ch.is_ascii_hexdigit()));
        }
    }
}
