//! PawnIO client (ADR 0002): detection for everyone, and — in the elevated helper
//! only — a read-only MSR channel through the official, signed `IntelMSR` module.
//!
//! We talk to the driver **only through its device IOCTLs** (the interface the
//! PawnIO GPL exception covers); PawnIOLib.dll is not used. The single module
//! function this code can ever call is `ioctl_read_msr`: there is no generic
//! "execute" entry point, so nothing here can write an MSR, a fan, a voltage or a
//! power limit. Module blobs are accepted only when their SHA-256 matches a known
//! official release.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use serde::Serialize;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, GENERIC_READ,
    GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_MANAGER_CONNECT,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
};

/// `CTL_CODE(41394, fn, METHOD_BUFFERED, FILE_ANY_ACCESS)` from PawnIO's `pawnio_um.h`.
const fn ctl(function: u32) -> u32 {
    (41394 << 16) | (function << 2)
}
const IOCTL_PIO_LOAD_BINARY: u32 = ctl(0x821);
const IOCTL_PIO_EXECUTE_FN: u32 = ctl(0x841);
/// The function-name field of an execute request.
const FN_NAME_LENGTH: usize = 32;
/// The only module function NANOFreeDisk calls.
const READ_MSR: &str = "ioctl_read_msr";

/// Official module builds we accept (PawnIO.Modules release, SHA-256).
pub const KNOWN_MODULES: &[(&str, &str, &str)] = &[(
    "IntelMSR",
    "0.2.11",
    "d6ed85d65ab17a22f813ef98207d6d537155ee2ded5976a21cb48413c9b92e5f",
)];

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// What the UI shows about Advanced sensors. Works without admin rights.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PawnStatus {
    /// The PawnIO driver service exists.
    pub installed: bool,
    /// The driver is loaded.
    pub running: bool,
    pub version: Option<String>,
    /// Memory Integrity (HVCI) is on — PawnIO is expected to work, but if the
    /// driver does not load this is the first thing to check.
    pub memory_integrity: bool,
    /// A known IntelMSR module was found next to the app or in its data folder.
    pub intel_module: bool,
    /// This process can open the device (only true when elevated).
    pub accessible: bool,
}

pub fn status() -> PawnStatus {
    let (installed, running) = service_state();
    PawnStatus {
        installed,
        running,
        version: uninstall_version(),
        memory_integrity: hvci_enabled(),
        intel_module: find_module("IntelMSR").is_some(),
        accessible: running && PawnIo::open().is_ok(),
    }
}

fn service_state() -> (bool, bool) {
    // SAFETY: connect-only SCM handle and a query-only service handle, both closed.
    unsafe {
        let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return (false, false);
        }
        let name = wide("PawnIO");
        let svc = OpenServiceW(scm, name.as_ptr(), SERVICE_QUERY_STATUS);
        let out = if svc.is_null() {
            (false, false)
        } else {
            let mut st = SERVICE_STATUS::default();
            let running =
                QueryServiceStatus(svc, &mut st) != 0 && st.dwCurrentState == SERVICE_RUNNING;
            CloseServiceHandle(svc);
            (true, running)
        };
        CloseServiceHandle(scm);
        out
    }
}

fn reg_dword(path: &str, value: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
    };
    let (k, v) = (wide(path), wide(value));
    let mut d = 0u32;
    let mut len = 4u32;
    // SAFETY: 4-byte out buffer.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut d as *mut u32).cast(),
            &mut len,
        )
    };
    (rc == 0).then_some(d)
}

fn hvci_enabled() -> bool {
    reg_dword(
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity",
        "Enabled",
    ) == Some(1)
}

fn uninstall_version() -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    let k = wide(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO");
    let v = wide("DisplayVersion");
    let mut buf = [0u16; 64];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: buffer size in bytes.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// SHA-256 through Windows CNG.
pub fn sha256(data: &[u8]) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    // SAFETY: pseudo-handle algorithm, input / output buffers with their sizes.
    let st = unsafe {
        BCryptHash(
            BCRYPT_SHA256_ALG_HANDLE,
            std::ptr::null(),
            0,
            data.as_ptr(),
            data.len() as u32,
            out.as_mut_ptr(),
            32,
        )
    };
    (st >= 0).then_some(out)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Whether `blob` is a known official build of `module`.
pub fn is_known_module(module: &str, blob: &[u8]) -> bool {
    let Some(h) = sha256(blob).map(|h| hex(&h)) else {
        return false;
    };
    KNOWN_MODULES
        .iter()
        .any(|(m, _, sha)| *m == module && *sha == h)
}

/// Where module blobs are looked up: `<exe dir>\pawnio\`, then
/// `%LOCALAPPDATA%\NANOFreeDisk\pawnio\`. Only known hashes are ever loaded.
pub fn module_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        dirs.push(d.join("pawnio"));
    }
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(l).join("NANOFreeDisk").join("pawnio"));
    }
    dirs
}

/// The first known-good blob of `module`.
pub fn find_module(module: &str) -> Option<Vec<u8>> {
    module_dirs().into_iter().find_map(|d| {
        let blob = std::fs::read(d.join(format!("{module}.bin"))).ok()?;
        is_known_module(module, &blob).then_some(blob)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PawnError {
    NotInstalled,
    /// The device is admin-only.
    NeedsAdmin,
    ModuleMissing,
    /// The driver refused the module (wrong CPU vendor, unsigned…).
    ModuleRejected,
    Failed,
}

/// An executor handle with the IntelMSR module loaded.
#[derive(Debug)]
pub struct PawnIo {
    h: HANDLE,
}

// SAFETY: the device handle is usable from any thread; calls are serialized by
// the owner (the sensor hub).
unsafe impl Send for PawnIo {}

impl Drop for PawnIo {
    fn drop(&mut self) {
        // SAFETY: opened in open().
        unsafe { CloseHandle(self.h) };
    }
}

impl PawnIo {
    fn open() -> Result<Self, PawnError> {
        let path = wide(r"\\?\GLOBALROOT\Device\PawnIO");
        // SAFETY: valid path, plain open.
        let h = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            // SAFETY: no arguments.
            return Err(match unsafe { GetLastError() } {
                ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => PawnError::NotInstalled,
                ERROR_ACCESS_DENIED => PawnError::NeedsAdmin,
                _ => PawnError::Failed,
            });
        }
        Ok(Self { h })
    }

    /// Open the device and load the official IntelMSR module.
    pub fn intel_msr() -> Result<Self, PawnError> {
        let blob = find_module("IntelMSR").ok_or(PawnError::ModuleMissing)?;
        let me = Self::open()?;
        let mut got = 0u32;
        // SAFETY: the blob is the input buffer; no output.
        let ok = unsafe {
            DeviceIoControl(
                me.h,
                IOCTL_PIO_LOAD_BINARY,
                blob.as_ptr().cast::<c_void>(),
                blob.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut got,
                std::ptr::null_mut(),
            )
        } != 0;
        if ok {
            Ok(me)
        } else {
            Err(PawnError::ModuleRejected)
        }
    }

    /// Read an MSR on the logical processor this thread runs on. The module's own
    /// allow-list refuses anything but the documented sensor registers.
    pub fn read_msr(&self, msr: u32) -> Option<u64> {
        let mut input = [0u8; FN_NAME_LENGTH + 8];
        input[..READ_MSR.len()].copy_from_slice(READ_MSR.as_bytes());
        input[FN_NAME_LENGTH..].copy_from_slice(&u64::from(msr).to_le_bytes());
        let mut out = 0u64;
        let mut got = 0u32;
        // SAFETY: buffered IOCTL with fixed-size buffers.
        let ok = unsafe {
            DeviceIoControl(
                self.h,
                IOCTL_PIO_EXECUTE_FN,
                input.as_ptr().cast(),
                input.len() as u32,
                (&mut out as *mut u64).cast(),
                8,
                &mut got,
                std::ptr::null_mut(),
            )
        } != 0;
        (ok && got == 8).then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_codes_match_pawnio_um_h() {
        assert_eq!(IOCTL_PIO_LOAD_BINARY, 0xA1B2_2084);
        assert_eq!(IOCTL_PIO_EXECUTE_FN, 0xA1B2_2104);
        assert!(READ_MSR.len() < FN_NAME_LENGTH);
    }

    #[test]
    fn module_hash_check() {
        assert_eq!(
            hex(&sha256(b"abc").expect("cng")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(!is_known_module("IntelMSR", b"not a module"));
        assert!(!is_known_module("Unknown", b""));
    }

    #[test]
    fn status_never_needs_admin() {
        let s = status();
        if !s.installed {
            assert!(!s.running && !s.accessible);
        }
    }
}
