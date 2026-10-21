//! Mounted volumes for the drive overview (PLAN §4.1, Phase 2).
//!
//! On Windows every drive letter is listed with `GetDriveTypeW`,
//! `GetVolumeInformationW` and `GetDiskFreeSpaceExW`. Drives without media (empty card
//! readers, DVD drives) are listed with `ready == false` and no sizes. On macOS
//! `getfsstat` lists `/` (the startup disk: the APFS system volume, sized like its
//! container) and the volumes Finder shows under `/Volumes`; the system volume
//! group's own hidden volumes (`/System/Volumes/*`) are left out. Other platforms
//! report `/` only, without sizes.

use std::path::PathBuf;

/// What kind of device a volume lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VolumeKind {
    Fixed,
    Removable,
    Network,
    Optical,
    RamDisk,
    Unknown,
}

/// One mounted volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// Mount path, e.g. `C:\`.
    pub root: PathBuf,
    /// Volume label (may be empty).
    pub label: String,
    /// File system name, e.g. `NTFS`, `ReFS`, `exFAT` (empty when unknown).
    pub fs: String,
    pub kind: VolumeKind,
    /// Total size in bytes (0 when unknown).
    pub total: u64,
    /// Free bytes available to the current user (quotas applied).
    pub available: u64,
    /// Free bytes on the volume.
    pub free: u64,
    /// Media present and readable.
    pub ready: bool,
    /// Holds the running Windows installation.
    pub system: bool,
}

/// List mounted volumes in drive-letter order.
#[must_use]
pub fn list() -> Vec<Volume> {
    imp::list()
}

/// `Some(true)` when the drive holding `path` is a spinning disk (it incurs a seek
/// penalty), `Some(false)` for SSD / NVMe, `None` when unknown (network drives,
/// other platforms). Used to read one file at a time from hard disks.
#[must_use]
pub fn seek_penalty(path: &std::path::Path) -> Option<bool> {
    imp::seek_penalty(path)
}

#[cfg(windows)]
mod imp {
    use std::env;
    use std::path::PathBuf;
    use std::ptr;

    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    use super::{Volume, VolumeKind};

    // GetDriveTypeW results (WinBase.h); kept local to avoid another feature flag.
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;

    pub(super) fn list() -> Vec<Volume> {
        // SAFETY: no arguments.
        let mask = unsafe { GetLogicalDrives() };
        let system_drive = env::var("SystemDrive")
            .unwrap_or_else(|_| "C:".to_owned())
            .to_ascii_uppercase();
        (0..26u8)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| {
                let letter = char::from(b'A' + i);
                volume(letter, system_drive.starts_with(letter))
            })
            .collect()
    }

    fn volume(letter: char, system: bool) -> Volume {
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
        // SAFETY: `wide` is a NUL-terminated root path.
        let kind = match unsafe { GetDriveTypeW(wide.as_ptr()) } {
            DRIVE_FIXED => VolumeKind::Fixed,
            DRIVE_REMOVABLE => VolumeKind::Removable,
            DRIVE_REMOTE => VolumeKind::Network,
            DRIVE_CDROM => VolumeKind::Optical,
            DRIVE_RAMDISK => VolumeKind::RamDisk,
            _ => VolumeKind::Unknown,
        };
        let mut label = [0u16; 261];
        let mut fs = [0u16; 261];
        // SAFETY: both buffers are writable and their lengths are passed; the optional
        // out-parameters are null.
        let ready = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        } != 0;
        let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
        if ready {
            // SAFETY: the three out-pointers are valid, writable u64s.
            let ok = unsafe {
                GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free)
            };
            if ok == 0 {
                (available, total, free) = (0, 0, 0);
            }
        }
        Volume {
            root: PathBuf::from(root),
            label: from_wide(&label),
            fs: from_wide(&fs),
            kind,
            total,
            available,
            free,
            ready,
            system,
        }
    }

    pub(super) fn seek_penalty(path: &std::path::Path) -> Option<bool> {
        use std::ffi::c_void;

        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        use windows_sys::Win32::System::IO::DeviceIoControl;
        use windows_sys::Win32::System::Ioctl::{
            IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
            StorageDeviceSeekPenaltyProperty,
        };

        let text = path.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        let letter = text.chars().next().filter(char::is_ascii_alphabetic)?;
        if text.chars().nth(1) != Some(':') {
            return None;
        }
        let device: Vec<u16> = format!(r"\\.\{letter}:")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: NUL-terminated path; no access rights requested (queries only).
        let h = unsafe {
            CreateFileW(
                device.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE || h.is_null() {
            return None;
        }
        let q = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceSeekPenaltyProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        // DEVICE_SEEK_PENALTY_DESCRIPTOR: Version, Size (u32 each), IncursSeekPenalty (BOOLEAN)
        let mut out = [0u8; 12];
        let mut got = 0u32;
        // SAFETY: input and output buffers are valid for their lengths; synchronous.
        let ok = unsafe {
            DeviceIoControl(
                h,
                IOCTL_STORAGE_QUERY_PROPERTY,
                (&raw const q).cast::<c_void>(),
                size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                out.as_mut_ptr().cast(),
                out.len() as u32,
                &mut got,
                ptr::null_mut(),
            )
        };
        // SAFETY: `h` is a handle this function opened.
        unsafe { CloseHandle(h) };
        (ok != 0 && got >= 9).then(|| out[8] != 0)
    }

    fn from_wide(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::{Path, PathBuf};

    use super::{Volume, VolumeKind};

    /// Text of a NUL-terminated `char` array.
    fn text(raw: &[libc::c_char]) -> String {
        let bytes: Vec<u8> = raw
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn mounts() -> Vec<libc::statfs> {
        // SAFETY: a null buffer only asks for the number of mounted file systems.
        let n = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
        let Ok(n) = usize::try_from(n) else {
            return Vec::new();
        };
        // room for a few more mounted meanwhile
        let mut buf: Vec<libc::statfs> = Vec::with_capacity(n + 8);
        let bytes = buf.capacity() * std::mem::size_of::<libc::statfs>();
        let Ok(bytes) = libc::c_int::try_from(bytes) else {
            return Vec::new();
        };
        // SAFETY: `buf` has room for `bytes` bytes of `statfs` records; the kernel
        // writes at most that many and returns how many records it filled.
        let got = unsafe { libc::getfsstat(buf.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
        let Ok(got) = usize::try_from(got) else {
            return Vec::new();
        };
        // SAFETY: the first `got` records were initialised by the kernel.
        unsafe { buf.set_len(got.min(buf.capacity())) };
        buf
    }

    /// Finder's name of the startup disk: the `/Volumes` entry linking to `/`.
    fn startup_name() -> String {
        std::fs::read_dir("/Volumes")
            .into_iter()
            .flatten()
            .flatten()
            .find(|e| std::fs::read_link(e.path()).is_ok_and(|t| t == Path::new("/")))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub(super) fn list() -> Vec<Volume> {
        let mut out = Vec::new();
        for m in mounts() {
            let root = text(&m.f_mntonname);
            let fs = text(&m.f_fstypename);
            let flags = i64::from(m.f_flags);
            let system = root == "/";
            let browsable = flags & i64::from(libc::MNT_DONTBROWSE) == 0;
            if !system && (!browsable || !root.starts_with("/Volumes/")) {
                continue;
            }
            if matches!(fs.as_str(), "devfs" | "autofs" | "nullfs") {
                continue;
            }
            let kind = if flags & i64::from(libc::MNT_LOCAL) == 0 {
                VolumeKind::Network
            } else if matches!(fs.as_str(), "cd9660" | "udf" | "cddafs") {
                VolumeKind::Optical
            } else if system {
                VolumeKind::Fixed
            } else if matches!(fs.as_str(), "msdos" | "exfat") {
                VolumeKind::Removable
            } else {
                VolumeKind::Fixed
            };
            let block = u64::from(m.f_bsize);
            let label = if system {
                startup_name()
            } else {
                Path::new(&root)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            out.push(Volume {
                root: PathBuf::from(&root),
                label,
                fs: fs.to_uppercase(),
                kind,
                total: m.f_blocks.saturating_mul(block),
                available: m.f_bavail.saturating_mul(block),
                free: m.f_bfree.saturating_mul(block),
                ready: true,
                system,
            });
        }
        if !out.iter().any(|v| v.system) {
            out.insert(0, fallback());
        }
        // the startup disk first, then by name
        out.sort_by(|a, b| b.system.cmp(&a.system).then_with(|| a.root.cmp(&b.root)));
        out
    }

    fn fallback() -> Volume {
        Volume {
            root: PathBuf::from("/"),
            label: String::new(),
            fs: String::new(),
            kind: VolumeKind::Fixed,
            total: 0,
            available: 0,
            free: 0,
            ready: true,
            system: true,
        }
    }

    pub(super) fn seek_penalty(_path: &Path) -> Option<bool> {
        None
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
mod imp {
    use std::path::PathBuf;

    use super::{Volume, VolumeKind};

    pub(super) fn list() -> Vec<Volume> {
        vec![Volume {
            root: PathBuf::from("/"),
            label: String::new(),
            fs: String::new(),
            kind: VolumeKind::Fixed,
            total: 0,
            available: 0,
            free: 0,
            ready: true,
            system: true,
        }]
    }

    pub(super) fn seek_penalty(_path: &std::path::Path) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_system_volume() {
        let vols = list();
        assert!(vols.iter().any(|v| v.system && v.ready), "{vols:?}");
        #[cfg(target_os = "macos")]
        {
            let sys = vols.iter().find(|v| v.system).expect("startup disk");
            assert_eq!(sys.root, std::path::Path::new("/"));
            assert!(sys.total > 0 && sys.free <= sys.total, "{sys:?}");
            assert!(vols.iter().all(|v| !v.root.starts_with("/System/Volumes")));
        }
        #[cfg(windows)]
        {
            let sys = vols.iter().find(|v| v.system).expect("system volume");
            assert!(sys.total > 0 && sys.free <= sys.total);
            assert!(!sys.fs.is_empty());
            // answered for local drives (SSD or HDD), never for nonsense
            let _ = seek_penalty(&sys.root);
            assert_eq!(seek_penalty(std::path::Path::new(r"\\nas\share")), None);
        }
    }
}
