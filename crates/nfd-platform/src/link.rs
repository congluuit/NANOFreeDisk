//! Replacing a duplicate by a link to the copy that stays (Phase 5, Duplicates).
//!
//! * [`hard_link`] — `CreateHardLinkW`: one more name for the same data (NTFS, same
//!   volume, at most 1023 names). Both paths then **share** contents, attributes,
//!   permissions and times.
//! * [`clone_file`] — `FSCTL_DUPLICATE_EXTENTS_TO_FILE` (ReFS, Dev Drive): a new file
//!   whose clusters are shared copy-on-write with the source; changing either later
//!   never affects the other.
//!
//! [`mode`] tells which of the two a volume supports. On macOS APFS volumes clone
//! with `clonefile(2)` (what Finder's Duplicate and `cp -c` do) and HFS+ volumes
//! hard-link. [`clone_info`] tells copies that *already* share their blocks
//! (APFS clones): deleting one of them frees nothing.

use std::io;
use std::path::Path;

/// How a duplicate can be replaced on a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkMode {
    /// Block clone (ReFS / Dev Drive): independent files sharing clusters.
    Clone,
    /// Hard link (NTFS): one file with two names.
    Hardlink,
}

/// NTFS limit of names per file.
pub const MAX_HARD_LINKS: u32 = 1023;

/// What the volume holding `path` supports (`None`: FAT/exFAT, network, unknown).
#[must_use]
pub fn mode(path: &Path) -> Option<LinkMode> {
    imp::mode(path)
}

/// Add `link` as another name of the file `existing` (same volume).
///
/// # Errors
/// The OS error (other volume, file system without hard links, too many links,
/// `link` exists).
pub fn hard_link(existing: &Path, link: &Path) -> io::Result<()> {
    std::fs::hard_link(existing, link)
}

/// Whether a file shares its data blocks with other files (APFS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CloneInfo {
    /// Identifies the data stream: *pure* clones of each other have the same id.
    pub id: Option<u64>,
    /// The file may share some blocks with another file (it was cloned, and maybe
    /// changed since): deleting it may free less than its size.
    pub may_share_blocks: bool,
}

/// [`CloneInfo`] of `path` (link not followed); `None` where the file system does
/// not say (Windows, HFS+, FAT…).
#[must_use]
pub fn clone_info(path: &Path) -> Option<CloneInfo> {
    imp::clone_info(path)
}

/// Create `dst` (must not exist) as a block clone of `src` on the same ReFS volume
/// (macOS: APFS), with the same last-write time.
///
/// # Errors
/// The OS error; a half-made `dst` is removed.
pub fn clone_file(src: &Path, dst: &Path) -> io::Result<()> {
    let r = imp::clone_file(src, dst);
    if r.is_err() {
        let _ = std::fs::remove_file(dst);
    }
    r
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_SHARE_READ, GetDiskFreeSpaceW, GetVolumeInformationW, GetVolumePathNameW,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        DUPLICATE_EXTENTS_DATA, FSCTL_DUPLICATE_EXTENTS_TO_FILE, FSCTL_SET_SPARSE,
    };

    use super::{CloneInfo, LinkMode};

    /// ReFS does not say which files share clusters.
    pub(super) fn clone_info(_path: &Path) -> Option<CloneInfo> {
        None
    }

    /// `FILE_SUPPORTS_BLOCK_REFCOUNTING`
    const BLOCK_REFCOUNTING: u32 = 0x0800_0000;
    /// `FILE_SUPPORTS_HARD_LINKS`
    const HARD_LINKS: u32 = 0x0040_0000;
    /// Bytes cloned per call (a multiple of every cluster size, below 4 GiB).
    const CHUNK: u64 = 1 << 30;

    fn wide(p: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    /// `C:\` of `path`, from the OS (mount points and `\\?\` handled).
    fn volume_root(path: &Path) -> Option<Vec<u16>> {
        let name = wide(path);
        let mut root = vec![0u16; 1024];
        // SAFETY: NUL-terminated input, output buffer of the given length.
        let ok = unsafe { GetVolumePathNameW(name.as_ptr(), root.as_mut_ptr(), root.len() as u32) };
        (ok != 0).then_some(root)
    }

    fn flags_and_fs(root: &[u16]) -> Option<(u32, String)> {
        let mut flags = 0u32;
        let mut fs = [0u16; 64];
        // SAFETY: NUL-terminated root; optional outputs null; fs buffer length passed.
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut flags,
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        let len = fs.iter().position(|&c| c == 0).unwrap_or(fs.len());
        Some((flags, String::from_utf16_lossy(&fs[..len])))
    }

    pub(super) fn mode(path: &Path) -> Option<LinkMode> {
        let root = volume_root(path)?;
        let (flags, fs) = flags_and_fs(&root)?;
        if flags & BLOCK_REFCOUNTING != 0 {
            Some(LinkMode::Clone)
        } else if flags & HARD_LINKS != 0 || fs.eq_ignore_ascii_case("NTFS") {
            Some(LinkMode::Hardlink)
        } else {
            None
        }
    }

    fn cluster_size(path: &Path) -> io::Result<u64> {
        let root = volume_root(path).ok_or_else(io::Error::last_os_error)?;
        let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
        // SAFETY: NUL-terminated root, four valid out-pointers.
        let ok =
            unsafe { GetDiskFreeSpaceW(root.as_ptr(), &mut spc, &mut bps, &mut free, &mut total) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(u64::from(spc) * u64::from(bps))
    }

    fn ioctl(file: &File, code: u32, input: *const c_void, len: u32) -> io::Result<()> {
        let mut got = 0u32;
        // SAFETY: `input` points at `len` readable bytes owned by the caller; no output.
        let ok = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                code,
                input,
                len,
                ptr::null_mut(),
                0,
                &mut got,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn clone_file(src: &Path, dst: &Path) -> io::Result<()> {
        let source = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(src)?;
        let meta = source.metadata()?;
        let size = meta.len();
        let target = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dst)?;
        // the target must be sparse when the source is, and as long as the source
        // before extents can be duplicated into it
        if std::os::windows::fs::MetadataExt::file_attributes(&meta) & 0x200 != 0 {
            ioctl(&target, FSCTL_SET_SPARSE, ptr::null(), 0)?;
        }
        target.set_len(size)?;
        let cluster = cluster_size(dst)?.max(1);
        let rounded = size.div_ceil(cluster) * cluster;
        let mut offset = 0u64;
        while offset < rounded {
            let count = CHUNK.min(rounded - offset);
            let data = DUPLICATE_EXTENTS_DATA {
                FileHandle: source.as_raw_handle(),
                SourceFileOffset: offset as i64,
                TargetFileOffset: offset as i64,
                ByteCount: count as i64,
            };
            ioctl(
                &target,
                FSCTL_DUPLICATE_EXTENTS_TO_FILE,
                (&raw const data).cast(),
                size_of::<DUPLICATE_EXTENTS_DATA>() as u32,
            )?;
            offset += count;
        }
        target.set_modified(meta.modified()?)?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::CString;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use super::{CloneInfo, LinkMode};

    // <sys/clonefile.h>
    const CLONE_NOFOLLOW: u32 = 0x0001;
    // <sys/attr.h>, ATTR_CMNEXT_EXT_FLAGS
    const EF_MAY_SHARE_BLOCKS: u64 = 0x0000_0001;

    fn c_path(path: &Path) -> io::Result<CString> {
        CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))
    }

    fn fs_type(path: &Path) -> Option<String> {
        let c = c_path(path).ok()?;
        // SAFETY: an all-zero `statfs` is a valid out buffer.
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: `c` is NUL-terminated; the kernel fills `st` on success.
        if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        let bytes: Vec<u8> = st
            .f_fstypename
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as u8)
            .collect();
        Some(String::from_utf8_lossy(&bytes).to_lowercase())
    }

    pub(super) fn mode(path: &Path) -> Option<LinkMode> {
        match fs_type(path)?.as_str() {
            "apfs" => Some(LinkMode::Clone),
            "hfs" => Some(LinkMode::Hardlink),
            _ => None,
        }
    }

    pub(super) fn clone_file(src: &Path, dst: &Path) -> io::Result<()> {
        let (s, d) = (c_path(src)?, c_path(dst)?);
        // SAFETY: both paths are NUL-terminated; clonefile never overwrites `dst`.
        if unsafe { libc::clonefile(s.as_ptr(), d.as_ptr(), CLONE_NOFOLLOW) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let modified = std::fs::symlink_metadata(src)?.modified()?;
        std::fs::File::options()
            .write(true)
            .open(dst)?
            .set_modified(modified)
    }

    /// What `getattrlist` writes for the attributes asked in [`clone_info`]: the
    /// length, the attribute set actually returned, then the values in bit order.
    #[repr(C, packed(4))]
    struct Reply {
        length: u32,
        returned: [u32; 5],
        values: [u64; 2],
    }

    pub(super) fn clone_info(path: &Path) -> Option<CloneInfo> {
        let c = c_path(path).ok()?;
        let mut list = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: libc::ATTR_CMN_RETURNED_ATTRS,
            volattr: 0,
            dirattr: 0,
            fileattr: 0,
            forkattr: libc::ATTR_CMNEXT_CLONEID | libc::ATTR_CMNEXT_EXT_FLAGS,
        };
        let mut reply = Reply {
            length: 0,
            returned: [0; 5],
            values: [0; 2],
        };
        // SAFETY: `list` and `reply` are valid for the sizes given; the kernel
        // writes at most `size_of::<Reply>()` bytes and never follows a link.
        let rc = unsafe {
            libc::getattrlist(
                c.as_ptr(),
                (&raw mut list).cast(),
                (&raw mut reply).cast(),
                std::mem::size_of::<Reply>(),
                libc::FSOPT_NOFOLLOW | libc::FSOPT_ATTR_CMN_EXTENDED,
            )
        };
        if rc != 0 {
            return None;
        }
        let returned = reply.returned[4];
        if returned & (libc::ATTR_CMNEXT_CLONEID | libc::ATTR_CMNEXT_EXT_FLAGS) == 0 {
            return None;
        }
        let values = reply.values;
        let mut next = values.iter().copied();
        let id = if returned & libc::ATTR_CMNEXT_CLONEID != 0 {
            next.next().filter(|&id| id != 0)
        } else {
            None
        };
        let ext = if returned & libc::ATTR_CMNEXT_EXT_FLAGS != 0 {
            next.next().unwrap_or(0)
        } else {
            0
        };
        Some(CloneInfo {
            id,
            may_share_blocks: ext & EF_MAY_SHARE_BLOCKS != 0,
        })
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
mod imp {
    use std::io;
    use std::path::Path;

    use super::{CloneInfo, LinkMode};

    pub(super) fn mode(_path: &Path) -> Option<LinkMode> {
        Some(LinkMode::Hardlink)
    }

    pub(super) fn clone_file(_src: &Path, _dst: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "block clone needs ReFS (Windows) or APFS (macOS)",
        ))
    }

    pub(super) fn clone_info(_path: &Path) -> Option<CloneInfo> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_links_share_data() {
        let dir = tempfile::tempdir().expect("tmp");
        let a = dir.path().join("a.bin");
        std::fs::write(&a, b"shared").expect("write");
        hard_link(&a, &dir.path().join("b.bin")).expect("link");
        assert_eq!(
            std::fs::read(dir.path().join("b.bin")).expect("read"),
            b"shared"
        );
        assert!(
            hard_link(&a, &dir.path().join("b.bin")).is_err(),
            "never overwrites"
        );
        let info = crate::fsinfo::file_info(&a).expect("info");
        if let Some(n) = info.link_count {
            assert_eq!(n, 2);
        }
    }

    #[cfg(windows)]
    #[test]
    fn the_temp_volume_has_a_mode() {
        let dir = tempfile::tempdir().expect("tmp");
        // NTFS (hard links) or ReFS / Dev Drive (block clone) on a dev machine
        assert!(mode(dir.path()).is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apfs_clones_share_their_data() {
        let dir = tempfile::tempdir().expect("tmp");
        if mode(dir.path()) != Some(LinkMode::Clone) {
            eprintln!("skipping: not APFS");
            return;
        }
        let a = dir.path().join("a.bin");
        std::fs::write(&a, vec![3u8; 200_000]).expect("write");
        let b = dir.path().join("b.bin");
        clone_file(&a, &b).expect("clonefile");
        assert_eq!(std::fs::read(&b).expect("read"), vec![3u8; 200_000]);
        let c = dir.path().join("c.bin");
        std::fs::write(&c, vec![3u8; 200_000]).expect("write");
        let (ia, ib, ic) = (clone_info(&a), clone_info(&b), clone_info(&c));
        eprintln!("clone info: {ia:?} {ib:?} {ic:?}");
        if let (Some(ia), Some(ib), Some(ic)) = (ia, ib, ic) {
            if ia.id.is_some() {
                assert_eq!(ia.id, ib.id, "pure clones have one clone id");
                assert_ne!(ia.id, ic.id, "a plain copy has its own");
            }
        }
    }

    #[test]
    fn clone_never_leaves_half_a_file() {
        let dir = tempfile::tempdir().expect("tmp");
        let a = dir.path().join("a.bin");
        std::fs::write(&a, vec![7u8; 100_000]).expect("write");
        let b = dir.path().join("b.bin");
        match clone_file(&a, &b) {
            // ReFS / Dev Drive
            Ok(()) => assert_eq!(std::fs::read(&b).expect("read"), vec![7u8; 100_000]),
            // NTFS and others refuse; nothing is left behind
            Err(_) => assert!(!b.exists()),
        }
    }
}
