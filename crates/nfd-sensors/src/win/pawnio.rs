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
/// The module functions NANOFreeDisk calls — reads only. `ioctl_read_smn` writes
/// the SMN *index* register of the AMD host bridge and reads the data register
/// (the same access k10temp and LibreHardwareMonitor use); it changes no setting.
const READ_MSR: &str = "ioctl_read_msr";
const READ_SMN: &str = "ioctl_read_smn";

/// Official module builds we accept (PawnIO.Modules release 0.2.11, SHA-256).
pub const KNOWN_MODULES: &[(&str, &str, &str)] = &[
    (
        "IntelMSR",
        "0.2.11",
        "d6ed85d65ab17a22f813ef98207d6d537155ee2ded5976a21cb48413c9b92e5f",
    ),
    (
        "AMDFamily17",
        "0.2.11",
        "dae74615761b78bdf064dfb3e136252ddcc6fc727d88f14738d0e5800d427a91",
    ),
];

/// The official downloads NANOFreeDisk may fetch when the user asks it to set up
/// Advanced sensors: (url, SHA-256). Pinned — a new upstream release is adopted
/// only by changing these after checking it (ADR 0002).
pub const SETUP_DOWNLOAD: (&str, &str) = (
    "https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe",
    "1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032",
);
pub const MODULES_DOWNLOAD: (&str, &str) = (
    "https://github.com/namazso/PawnIO.Modules/releases/download/0.2.11/release_0_2_11.zip",
    "43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4",
);
/// Upper bounds for the two downloads (3.4 MB and 70 KB today).
pub const SETUP_MAX_BYTES: usize = 16 << 20;
pub const MODULES_MAX_BYTES: usize = 4 << 20;

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
    /// The known module for this CPU (IntelMSR / AMDFamily17) was found next to
    /// the app or in its data folder.
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
        intel_module: find_module(cpu_module()).is_some(),
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

/// The module for this machine's CPU.
pub fn cpu_module() -> &'static str {
    if nfd_hwinfo::cpu::native().vendor == "AMD" {
        "AMDFamily17"
    } else {
        "IntelMSR"
    }
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

/// Whether `data` hashes to the lowercase hex `sha`.
pub fn sha256_matches(data: &[u8], sha: &str) -> bool {
    sha256(data).is_some_and(|h| hex(&h) == sha)
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

/// `<module>.bin` out of the official module archive (a plain zip), wherever it
/// sits in the archive. Stored or deflated entries only; `None` for anything
/// malformed, encrypted or larger than a module can be.
pub fn extract_module(zip: &[u8], module: &str) -> Option<Vec<u8>> {
    const MAX_MODULE: usize = 1 << 20;
    let u16_at = |at: usize| -> Option<usize> {
        Some(u16::from_le_bytes(zip.get(at..at + 2)?.try_into().ok()?) as usize)
    };
    let u32_at = |at: usize| -> Option<usize> {
        Some(u32::from_le_bytes(zip.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    // end of central directory: 22 bytes plus a comment of up to 64 KB
    let lowest = zip.len().saturating_sub(22 + 0xFFFF);
    let eocd = (lowest..=zip.len().checked_sub(22)?)
        .rev()
        .find(|&i| zip[i..i + 4] == [0x50, 0x4B, 0x05, 0x06])?;
    let entries = u16_at(eocd + 10)?;
    let mut at = u32_at(eocd + 16)?;
    let want = format!("{module}.bin");
    for _ in 0..entries {
        if zip.get(at..at + 4)? != [0x50, 0x4B, 0x01, 0x02] {
            return None;
        }
        let flags = u16_at(at + 8)?;
        let method = u16_at(at + 10)?;
        let packed = u32_at(at + 20)?;
        let size = u32_at(at + 24)?;
        let (name_len, extra_len, comment_len) =
            (u16_at(at + 28)?, u16_at(at + 30)?, u16_at(at + 32)?);
        let local = u32_at(at + 42)?;
        let name = zip.get(at + 46..at + 46 + name_len)?;
        at += 46 + name_len + extra_len + comment_len;
        let base = name
            .rsplit(|&c| c == b'/' || c == b'\\')
            .next()
            .unwrap_or(name);
        if !base.eq_ignore_ascii_case(want.as_bytes()) {
            continue;
        }
        if flags & 1 != 0 || size > MAX_MODULE || packed > MAX_MODULE {
            return None; // encrypted, or not a module
        }
        if zip.get(local..local + 4)? != [0x50, 0x4B, 0x03, 0x04] {
            return None;
        }
        let start = local + 30 + u16_at(local + 26)? + u16_at(local + 28)?;
        let data = zip.get(start..start + packed)?;
        let out = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, MAX_MODULE).ok()?,
            _ => return None,
        };
        return (out.len() == size).then_some(out);
    }
    None
}

/// Save a known official module where [`find_module`] looks for it
/// (`%LOCALAPPDATA%\NANOFreeDisk\pawnio\`). Anything else is refused.
pub fn save_module(module: &str, blob: &[u8]) -> std::io::Result<PathBuf> {
    if !is_known_module(module, blob) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a known official module build",
        ));
    }
    let dir = std::env::var_os("LOCALAPPDATA")
        .map(|l| PathBuf::from(l).join("NANOFreeDisk").join("pawnio"))
        .ok_or_else(|| std::io::Error::other("LOCALAPPDATA is not set"))?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{module}.bin"));
    std::fs::write(&path, blob)?;
    Ok(path)
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
        Self::load("IntelMSR")
    }

    /// Open the device and load the official AMDFamily17 module (Zen 1–5).
    pub fn amd_family17() -> Result<Self, PawnError> {
        Self::load("AMDFamily17")
    }

    fn load(module: &str) -> Result<Self, PawnError> {
        let blob = find_module(module).ok_or(PawnError::ModuleMissing)?;
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
        self.read1(READ_MSR, u64::from(msr))
    }

    /// Read an AMD SMN register (AMDFamily17 only): temperatures, SVI telemetry.
    pub fn read_smn(&self, address: u32) -> Option<u32> {
        self.read1(READ_SMN, u64::from(address)).map(|v| v as u32)
    }

    /// One-in, one-out call of a read function from the fixed list above.
    fn read1(&self, function: &'static str, arg: u64) -> Option<u64> {
        let mut input = [0u8; FN_NAME_LENGTH + 8];
        input[..function.len()].copy_from_slice(function.as_bytes());
        input[FN_NAME_LENGTH..].copy_from_slice(&arg.to_le_bytes());
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
        assert!(READ_MSR.len() < FN_NAME_LENGTH && READ_SMN.len() < FN_NAME_LENGTH);
        assert!(!READ_MSR.contains("write") && !READ_SMN.contains("write"));
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

    /// A minimal zip: (name, data, deflate?) entries, CRCs left at 0 (not checked).
    fn zip_of(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data, deflate) in entries {
            let packed = if *deflate {
                miniz_oxide::deflate::compress_to_vec(data, 6)
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            out.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04, 20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 8]); // time, date, crc
            out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&packed);
            central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02, 20, 0, 20, 0, 0, 0]);
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 8]);
            central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]); // extra, comment, disk, attrs
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_at = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06, 0, 0, 0, 0]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_at.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn module_comes_out_of_the_archive() {
        let intel = vec![7u8; 5000];
        let amd = b"amd module".to_vec();
        let zip = zip_of(&[
            ("release/LpcIO.bin", b"other", false),
            ("release/IntelMSR.bin", &intel, true),
            ("AMDFamily17.bin", &amd, false),
        ]);
        assert_eq!(extract_module(&zip, "IntelMSR"), Some(intel));
        assert_eq!(extract_module(&zip, "AMDFamily17"), Some(amd));
        assert_eq!(extract_module(&zip, "RyzenSMU"), None, "not in the archive");
        assert_eq!(
            extract_module(&zip[..zip.len() - 3], "IntelMSR"),
            None,
            "cut"
        );
        assert_eq!(extract_module(b"PK", "IntelMSR"), None);
        let mut bad = zip.clone();
        let at = bad
            .windows(4)
            .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
            .expect("cd");
        bad[at] = 0;
        assert_eq!(extract_module(&bad, "IntelMSR"), None, "broken directory");
    }

    #[test]
    fn only_official_modules_are_saved() {
        let e = save_module("IntelMSR", b"not a module").expect_err("refused");
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
        assert!(sha256_matches(
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
        assert!(!sha256_matches(b"abd", SETUP_DOWNLOAD.1));
        for (url, sha) in [SETUP_DOWNLOAD, MODULES_DOWNLOAD] {
            assert!(url.starts_with("https://github.com/namazso/"), "{url}");
            assert_eq!(sha.len(), 64);
        }
    }

    #[test]
    fn status_never_needs_admin() {
        let s = status();
        if !s.installed {
            assert!(!s.running && !s.accessible);
        }
    }
}
