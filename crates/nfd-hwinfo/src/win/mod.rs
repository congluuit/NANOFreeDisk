//! Windows sources for the inventory. Every call here is read-only and works for a
//! standard user; anything that fails just leaves its field empty.

pub mod battery;
pub mod display;
pub mod gpu;
pub mod net;
pub mod os;
pub mod storage;
pub mod topology;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Registry::{
    HKEY, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RRF_RT_REG_QWORD, RRF_RT_REG_SZ, RegGetValueW,
};

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().to_owned()
}

/// Owned kernel handle, closed on drop.
#[derive(Debug)]
pub struct Handle(pub HANDLE);

// SAFETY: kernel handles may be used from any thread; the owner is single.
unsafe impl Send for Handle {}

impl Handle {
    pub fn valid(h: HANDLE) -> Option<Self> {
        (!h.is_null() && h != INVALID_HANDLE_VALUE).then_some(Self(h))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful open and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// A REG_SZ / REG_EXPAND_SZ value.
pub(crate) fn reg_string(root: HKEY, key: &str, value: &str) -> Option<String> {
    let (k, v) = (wide(key), wide(value));
    let mut buf = vec![0u16; 512];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: buffers are valid for `len` bytes; RegGetValueW NUL-terminates.
    let rc = unsafe {
        RegGetValueW(
            root,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    (rc == 0).then(|| from_wide(&buf)).filter(|s| !s.is_empty())
}

/// A REG_DWORD, REG_QWORD or (≤ 8-byte) REG_BINARY value as an integer.
pub(crate) fn reg_u64(root: HKEY, key: &str, value: &str) -> Option<u64> {
    let (k, v) = (wide(key), wide(value));
    let mut buf = [0u8; 8];
    let mut len = buf.len() as u32;
    // SAFETY: 8-byte buffer, length passed in.
    let rc = unsafe {
        RegGetValueW(
            root,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_DWORD | RRF_RT_REG_QWORD | RRF_RT_REG_BINARY,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    (rc == 0).then(|| u64::from_le_bytes(buf))
}

/// SMBIOS through `GetSystemFirmwareTable('RSMB')`.
pub fn smbios_blob() -> Option<Vec<u8>> {
    use windows_sys::Win32::System::SystemInformation::GetSystemFirmwareTable;
    const RSMB: u32 = u32::from_be_bytes(*b"RSMB");
    // SAFETY: a null buffer asks for the size.
    let size = unsafe { GetSystemFirmwareTable(RSMB, 0, std::ptr::null_mut(), 0) };
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: buffer of `size` bytes.
    let got = unsafe { GetSystemFirmwareTable(RSMB, 0, buf.as_mut_ptr().cast(), size) };
    (got > 0 && got <= size).then(|| {
        buf.truncate(got as usize);
        buf
    })
}
