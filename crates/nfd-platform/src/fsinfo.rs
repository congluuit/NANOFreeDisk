//! File metadata for the scanner.
//!
//! Two entry points, both of which never follow reparse points / symlinks — the
//! returned [`FileInfo`] always describes the link itself and [`LinkKind`] says what
//! kind of link it is, so the caller can decide not to descend:
//!
//! * [`read_dir`] lists a directory *with* metadata for every child. On Windows this is
//!   one `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` call per ~64 KiB of
//!   entries — allocation size, end of file, attributes, reparse tag and 128-bit file
//!   ID come straight from the directory index, without opening the children (so
//!   cloud placeholders are never touched). That is ~5× faster than opening every
//!   file. The listing does not carry the hard-link count, so
//!   [`FileInfo::link_count`] is `None` unless `exact_links` is requested. Children
//!   that need a closer look (mount-point vs junction, NTFS/WOF-compressed or sparse
//!   files) are probed individually with [`file_info`]. Filesystems without that
//!   information class (FAT, some network shares) fall back to `std::fs::read_dir` +
//!   [`file_info`] per child.
//! * [`file_info`] queries one entry. On Windows it opens the entry with
//!   `FILE_READ_ATTRIBUTES` + `FILE_FLAG_OPEN_REPARSE_POINT` (never hydrates cloud
//!   placeholders, never takes a sharing lock) and uses `GetFileInformationByHandle(Ex)`.
//!   Compressed / sparse / WOF files report their compressed size (the same number as
//!   `GetCompressedFileSizeW`). If the entry cannot be opened, its directory-listing
//!   data (`FindFirstFileExW`) is used and [`FileInfo::approximate`] is set.
//!
//! On Unix both use `lstat`: `st_blocks * 512` for the on-disk size, `st_nlink`,
//! `st_ino`, `st_dev`. On macOS `st_flags` adds hidden (`UF_HIDDEN`), compressed,
//! locked and SIP-restricted files, and iCloud / File Provider *dataless* files
//! (`SF_DATALESS`, [`FileFlags::OFFLINE`]: reading one downloads it). The APFS
//! system volume and its firmlinked Data volume count as one volume, so `/Users`
//! is walked from `/`; their other path, `/System/Volumes/Data`, is a
//! [`LinkKind::MountPoint`] (never walked twice).

use std::ffi::OsString;
use std::io;
use std::path::Path;

/// Platform-neutral attribute bits (mapped onto `nfd_core::model::Attrs` by the core).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FileFlags(pub u32);

impl FileFlags {
    pub const SYSTEM: Self = Self(1 << 0);
    pub const HIDDEN: Self = Self(1 << 1);
    pub const REPARSE: Self = Self(1 << 2);
    pub const COMPRESSED: Self = Self(1 << 3);
    pub const SPARSE: Self = Self(1 << 4);
    /// Cloud placeholder / offline storage: the data is not on this disk.
    pub const OFFLINE: Self = Self(1 << 5);
    pub const READONLY: Self = Self(1 << 6);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// What kind of link (name-surrogate reparse point) an entry is.
///
/// Reparse points that are *not* links — OneDrive placeholders, dedup, WOF
/// compression, app-exec aliases — are [`LinkKind::None`] with
/// [`FileFlags::REPARSE`] set; they are regular files/directories.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum LinkKind {
    #[default]
    None,
    /// File or directory symbolic link (any name-surrogate tag other than a mount point).
    Symlink,
    /// NTFS directory junction pointing at a path.
    Junction,
    /// Volume mount point (junction whose target is `\??\Volume{…}`).
    MountPoint,
}

/// Metadata of one filesystem entry, as seen without following links.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// Logical size (end of file) in bytes; 0 for directories and links.
    pub logical: u64,
    /// Bytes actually allocated on disk (compression/sparse aware); 0 for directories
    /// and links.
    pub on_disk: u64,
    /// Last-write time, Unix seconds.
    pub mtime: i64,
    pub flags: FileFlags,
    pub is_dir: bool,
    pub link: LinkKind,
    /// Number of hard links, or `None` when the source (a Windows directory listing)
    /// does not carry it.
    pub link_count: Option<u32>,
    /// Volume serial number (Windows) or `st_dev` (Unix).
    pub volume_id: u64,
    /// File ID unique within the volume (NTFS/ReFS `FILE_ID_128`, or the inode).
    pub file_id: Option<u128>,
    /// `true` when the entry could not be opened and the values come from a
    /// `FindFirstFile` listing (on-disk size = logical size, no file ID).
    pub approximate: bool,
}

/// One child returned by [`read_dir`].
#[derive(Debug)]
pub struct DirItem {
    pub name: OsString,
    /// Metadata, or why it could not be read (the child still exists).
    pub info: io::Result<FileInfo>,
}

/// Query metadata for `path` without following links.
///
/// # Errors
/// Returns the OS error when the entry can be neither opened nor listed
/// (it vanished, or the parent denies listing).
pub fn file_info(path: &Path) -> io::Result<FileInfo> {
    imp::file_info(path)
}

/// List `dir` with metadata for every child (excluding `.` and `..`), in no
/// particular order. With `exact_links`, every file is opened so that
/// [`FileInfo::link_count`] is always known (much slower on Windows).
///
/// # Errors
/// Returns the OS error when the directory itself cannot be opened or listed.
pub fn read_dir(dir: &Path, exact_links: bool) -> io::Result<Vec<DirItem>> {
    imp::read_dir(dir, exact_links)
}

/// `true` for macOS `st_flags` of a *dataless* file (iCloud Drive / File Provider:
/// the contents are in the cloud and reading them downloads them).
#[cfg(target_os = "macos")]
#[must_use]
pub fn macos_dataless(st_flags: u32) -> bool {
    st_flags & imp::macos::SF_DATALESS != 0
}

/// `std::fs::read_dir` + [`file_info`] per child: the portable path.
fn read_dir_probing(dir: &Path) -> io::Result<Vec<DirItem>> {
    std::fs::read_dir(dir)?
        .map(|child| {
            let child = child?;
            Ok(DirItem {
                info: file_info(&child.path()),
                name: child.file_name(),
            })
        })
        .collect()
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::ffi::{OsStr, OsString};
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;
    use std::{mem, ptr};

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_FUNCTION, ERROR_INVALID_LEVEL, ERROR_INVALID_PARAMETER,
        ERROR_NO_MORE_FILES, ERROR_NOT_SUPPORTED, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_COMPRESSED,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_OFFLINE,
        FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
        FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE,
        FILE_ATTRIBUTE_SYSTEM, FILE_ATTRIBUTE_TAG_INFO, FILE_COMPRESSION_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_EXTD_DIR_INFO,
        FILE_ID_INFO, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FileAttributeTagInfo,
        FileCompressionInfo, FileIdExtdDirectoryInfo, FileIdExtdDirectoryRestartInfo, FileIdInfo,
        FileStandardInfo, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW,
        GetFileInformationByHandle, GetFileInformationByHandleEx, MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
        OPEN_EXISTING, WIN32_FIND_DATAW,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_GET_REPARSE_POINT;
    use windows_sys::Win32::System::SystemServices::{
        IO_REPARSE_TAG_MOUNT_POINT, IO_REPARSE_TAG_SYMLINK, IO_REPARSE_TAG_WOF,
    };

    use super::{DirItem, FileFlags, FileInfo, LinkKind, read_dir_probing};

    /// Seconds between 1601-01-01 (FILETIME epoch) and 1970-01-01.
    const FILETIME_UNIX_DIFF_SECS: i64 = 11_644_473_600;
    /// Bit 29 of a reparse tag: the reparse point is a name surrogate (a link).
    const REPARSE_TAG_NAME_SURROGATE: u32 = 0x2000_0000;
    /// Directory-listing buffer size, in `u64`s (64 KiB, 8-byte aligned).
    const LIST_BUF_WORDS: usize = 64 * 1024 / 8;

    thread_local! {
        static LIST_BUF: RefCell<Vec<u64>> = RefCell::new(vec![0; LIST_BUF_WORDS]);
    }

    /// Owned Win32 handle closed on drop.
    struct Handle(HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a valid handle returned by `CreateFileW` and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub(super) fn file_info(path: &Path) -> io::Result<FileInfo> {
        let wide = to_wide(path);
        match open(&wide, FILE_READ_ATTRIBUTES) {
            Ok(handle) => from_handle(&handle),
            Err(open_err) => from_find_data(&wide).map_err(|_| open_err),
        }
    }

    pub(super) fn read_dir(dir: &Path, exact_links: bool) -> io::Result<Vec<DirItem>> {
        let handle = open(&to_wide(dir), FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)?;
        let volume_id = by_handle(&handle).map_or(0, |b| u64::from(b.dwVolumeSerialNumber));
        match list(&handle, dir, volume_id, exact_links) {
            Err(e) if is_unsupported(&e) => read_dir_probing(dir),
            other => other,
        }
    }

    fn is_unsupported(e: &io::Error) -> bool {
        e.raw_os_error().is_some_and(|code| {
            [
                ERROR_INVALID_PARAMETER,
                ERROR_INVALID_LEVEL,
                ERROR_NOT_SUPPORTED,
                ERROR_INVALID_FUNCTION,
            ]
            .iter()
            .any(|&c| c as i32 == code)
        })
    }

    /// Enumerate with `FileIdExtdDirectoryInfo`.
    fn list(
        handle: &Handle,
        dir: &Path,
        volume_id: u64,
        exact_links: bool,
    ) -> io::Result<Vec<DirItem>> {
        let name_offset = mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileName);
        let mut items = Vec::new();
        LIST_BUF.with_borrow_mut(|buf| {
            let bytes = buf.len() * 8;
            let mut class = FileIdExtdDirectoryRestartInfo;
            loop {
                // SAFETY: `buf` is a writable, 8-byte-aligned buffer of `bytes` bytes.
                let ok = unsafe {
                    GetFileInformationByHandleEx(
                        handle.0,
                        class,
                        buf.as_mut_ptr().cast(),
                        bytes as u32,
                    )
                };
                if ok == 0 {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                        return Ok(());
                    }
                    return Err(err);
                }
                class = FileIdExtdDirectoryInfo;

                let base: *const u8 = buf.as_ptr().cast();
                let mut offset = 0usize;
                while offset + name_offset <= bytes {
                    // SAFETY: the record header lies inside the buffer (checked above);
                    // `read_unaligned` tolerates any alignment.
                    let rec: FILE_ID_EXTD_DIR_INFO =
                        unsafe { ptr::read_unaligned(base.add(offset).cast()) };
                    let name_len = rec.FileNameLength as usize / 2;
                    let name_start = offset + name_offset;
                    if name_start + name_len * 2 > bytes {
                        break;
                    }
                    let name: Vec<u16> = (0..name_len)
                        // SAFETY: every name unit lies inside the buffer (checked above).
                        .map(|i| unsafe { ptr::read_unaligned(base.add(name_start + 2 * i).cast()) })
                        .collect();
                    if name != [u16::from(b'.')] && name != [u16::from(b'.'); 2] {
                        let name = OsString::from_wide(&name);
                        let info = from_record(&rec, dir, &name, volume_id, exact_links);
                        items.push(DirItem { name, info });
                    }
                    if rec.NextEntryOffset == 0 {
                        break;
                    }
                    offset += rec.NextEntryOffset as usize;
                }
            }
        })?;
        Ok(items)
    }

    fn from_record(
        rec: &FILE_ID_EXTD_DIR_INFO,
        dir: &Path,
        name: &OsStr,
        volume_id: u64,
        exact_links: bool,
    ) -> io::Result<FileInfo> {
        let attrs = rec.FileAttributes;
        let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY != 0;
        let tag = if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            rec.ReparsePointTag
        } else {
            0
        };
        let compressed = attrs & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) != 0
            || tag == IO_REPARSE_TAG_WOF;
        let needs_probe =
            tag == IO_REPARSE_TAG_MOUNT_POINT || (!is_dir && (compressed || exact_links));
        if needs_probe {
            // Mount point vs junction needs the reparse data; compressed sizes and link
            // counts need a handle.
            if let Ok(info) = file_info(&dir.join(name)) {
                return Ok(info);
            }
        }
        let link = if tag == 0 {
            LinkKind::None
        } else {
            link_kind(tag, || false)
        };
        let sized = !is_dir && link == LinkKind::None;
        Ok(FileInfo {
            logical: if sized {
                rec.EndOfFile.max(0) as u64
            } else {
                0
            },
            on_disk: if sized {
                rec.AllocationSize.max(0) as u64
            } else {
                0
            },
            mtime: filetime_to_unix(rec.LastWriteTime),
            flags: flags_from_attrs(attrs),
            is_dir,
            link,
            link_count: None,
            volume_id,
            file_id: Some(u128::from_le_bytes(rec.FileId.Identifier)),
            approximate: false,
        })
    }

    /// NUL-terminated UTF-16 path, `\\?\`-prefixed when it would exceed `MAX_PATH`.
    fn to_wide(path: &Path) -> Vec<u16> {
        let os: &OsStr = path.as_os_str();
        let mut wide: Vec<u16> = os.encode_wide().collect();
        let already_verbatim = wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16]);
        if wide.len() >= 248 && !already_verbatim && path.is_absolute() {
            let unc = wide.starts_with(&[b'\\' as u16, b'\\' as u16]);
            let prefix: &str = if unc { r"\\?\UNC" } else { r"\\?\" };
            let rest = if unc { &wide[1..] } else { &wide[..] };
            let mut out: Vec<u16> = prefix.encode_utf16().collect();
            out.extend(
                rest.iter()
                    .map(|&c| if c == b'/' as u16 { b'\\' as u16 } else { c }),
            );
            wide = out;
        }
        wide.push(0);
        wide
    }

    fn open(wide: &[u16], access: u32) -> io::Result<Handle> {
        // SAFETY: `wide` is NUL-terminated; all other arguments are plain values or null.
        let h = unsafe {
            CreateFileW(
                wide.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Handle(h))
        }
    }

    fn by_handle(handle: &Handle) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
        let mut bhfi = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `bhfi` is a valid, writable BY_HANDLE_FILE_INFORMATION.
        if unsafe { GetFileInformationByHandle(handle.0, &mut bhfi) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(bhfi)
    }

    /// `GetFileInformationByHandleEx` into a zeroed `T`.
    fn query<T: Copy>(handle: &Handle, class: i32) -> io::Result<T> {
        // SAFETY: every `T` used here is a plain-old-data `#[repr(C)]` windows-sys struct
        // for which all-zero bytes are a valid value.
        let mut out: T = unsafe { mem::zeroed() };
        // SAFETY: `out` is a writable buffer of exactly `size_of::<T>()` bytes matching `class`.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                handle.0,
                class,
                (&raw mut out).cast(),
                mem::size_of::<T>() as u32,
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(out)
        }
    }

    fn from_handle(handle: &Handle) -> io::Result<FileInfo> {
        let bhfi = by_handle(handle)?;
        let attrs = bhfi.dwFileAttributes;
        let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY != 0;
        let mut flags = flags_from_attrs(attrs);

        let mut link = LinkKind::None;
        let mut tag = 0;
        if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            tag = query::<FILE_ATTRIBUTE_TAG_INFO>(handle, FileAttributeTagInfo)
                .map_or(0, |t| t.ReparseTag);
            link = link_kind(tag, || mount_point_targets_volume(handle));
        }

        let (mut logical, mut on_disk) = (0, 0);
        if !is_dir && link == LinkKind::None {
            let std = query::<FILE_STANDARD_INFO>(handle, FileStandardInfo)?;
            logical = std.EndOfFile.max(0) as u64;
            on_disk = std.AllocationSize.max(0) as u64;
            let compressed = attrs & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) != 0
                || tag == IO_REPARSE_TAG_WOF;
            if compressed {
                if let Ok(c) = query::<FILE_COMPRESSION_INFO>(handle, FileCompressionInfo) {
                    on_disk = c.CompressedFileSize.max(0) as u64;
                }
                if tag == IO_REPARSE_TAG_WOF {
                    flags = flags.with(FileFlags::COMPRESSED);
                }
            }
        }

        // Same 128-bit ID space as directory listings; the 64-bit index is the
        // fallback for filesystems without FileIdInfo.
        let file_id = query::<FILE_ID_INFO>(handle, FileIdInfo).map_or_else(
            |_| (u128::from(bhfi.nFileIndexHigh) << 32) | u128::from(bhfi.nFileIndexLow),
            |id| u128::from_le_bytes(id.FileId.Identifier),
        );

        Ok(FileInfo {
            logical,
            on_disk,
            mtime: filetime_to_unix(
                (i64::from(bhfi.ftLastWriteTime.dwHighDateTime) << 32)
                    | i64::from(bhfi.ftLastWriteTime.dwLowDateTime),
            ),
            flags,
            is_dir,
            link,
            link_count: Some(bhfi.nNumberOfLinks.max(1)),
            volume_id: u64::from(bhfi.dwVolumeSerialNumber),
            file_id: Some(file_id),
            approximate: false,
        })
    }

    /// Fallback when the entry itself cannot be opened: use the parent's listing.
    fn from_find_data(wide: &[u16]) -> io::Result<FileInfo> {
        let mut data = WIN32_FIND_DATAW::default();
        // SAFETY: `wide` is NUL-terminated and `data` is a writable WIN32_FIND_DATAW,
        // which is what `FindExInfoBasic` fills.
        let h = unsafe {
            FindFirstFileExW(
                wide.as_ptr(),
                FindExInfoBasic,
                (&raw mut data).cast(),
                FindExSearchNameMatch,
                ptr::null(),
                0,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `h` is a valid search handle from FindFirstFileExW.
        unsafe { FindClose(h) };

        let attrs = data.dwFileAttributes;
        let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY != 0;
        let link = if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            // No handle, so a mount point cannot be told apart from a junction;
            // both are boundaries anyway.
            link_kind(data.dwReserved0, || false)
        } else {
            LinkKind::None
        };
        let size = (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow);
        let size = if is_dir || link != LinkKind::None {
            0
        } else {
            size
        };
        Ok(FileInfo {
            logical: size,
            on_disk: size,
            mtime: filetime_to_unix(
                (i64::from(data.ftLastWriteTime.dwHighDateTime) << 32)
                    | i64::from(data.ftLastWriteTime.dwLowDateTime),
            ),
            flags: flags_from_attrs(attrs),
            is_dir,
            link,
            link_count: None,
            volume_id: 0,
            file_id: None,
            approximate: true,
        })
    }

    fn flags_from_attrs(attrs: u32) -> FileFlags {
        let map = [
            (FILE_ATTRIBUTE_SYSTEM, FileFlags::SYSTEM),
            (FILE_ATTRIBUTE_HIDDEN, FileFlags::HIDDEN),
            (FILE_ATTRIBUTE_REPARSE_POINT, FileFlags::REPARSE),
            (FILE_ATTRIBUTE_COMPRESSED, FileFlags::COMPRESSED),
            (FILE_ATTRIBUTE_SPARSE_FILE, FileFlags::SPARSE),
            (FILE_ATTRIBUTE_OFFLINE, FileFlags::OFFLINE),
            (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FileFlags::OFFLINE),
            (FILE_ATTRIBUTE_RECALL_ON_OPEN, FileFlags::OFFLINE),
            (FILE_ATTRIBUTE_READONLY, FileFlags::READONLY),
        ];
        map.iter()
            .filter(|(bit, _)| attrs & bit != 0)
            .fold(FileFlags::default(), |acc, (_, f)| acc.with(*f))
    }

    fn link_kind(tag: u32, is_volume_mount: impl FnOnce() -> bool) -> LinkKind {
        match tag {
            IO_REPARSE_TAG_MOUNT_POINT if is_volume_mount() => LinkKind::MountPoint,
            IO_REPARSE_TAG_MOUNT_POINT => LinkKind::Junction,
            IO_REPARSE_TAG_SYMLINK => LinkKind::Symlink,
            t if t & REPARSE_TAG_NAME_SURROGATE != 0 => LinkKind::Symlink,
            _ => LinkKind::None,
        }
    }

    /// Read the mount-point reparse data and check whether its substitute name is a
    /// volume GUID path (`\??\Volume{…}\`) rather than a directory path.
    fn mount_point_targets_volume(handle: &Handle) -> bool {
        let mut buf = vec![0u8; MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
        let mut returned = 0u32;
        // SAFETY: `buf` is writable for `buf.len()` bytes; no input buffer; synchronous call.
        let ok = unsafe {
            DeviceIoControl(
                handle.0,
                FSCTL_GET_REPARSE_POINT,
                ptr::null(),
                0,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return false;
        }
        substitute_name(&buf[..returned as usize])
            .is_some_and(|name| name.starts_with(r"\??\Volume{"))
    }

    /// Parse `REPARSE_DATA_BUFFER.MountPointReparseBuffer.SubstituteName`.
    fn substitute_name(buf: &[u8]) -> Option<String> {
        // Header: tag u32, data len u16, reserved u16; then offset u16, len u16, … ;
        // the path buffer starts at byte 16.
        let u16_at = |i: usize| Some(u16::from_le_bytes([*buf.get(i)?, *buf.get(i + 1)?]));
        let offset = usize::from(u16_at(8)?);
        let len = usize::from(u16_at(10)?);
        let start = 16 + offset;
        let bytes = buf.get(start..start + len)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Some(String::from_utf16_lossy(&units))
    }

    /// FILETIME ticks (100 ns since 1601) → Unix seconds.
    fn filetime_to_unix(ticks: i64) -> i64 {
        ticks.div_euclid(10_000_000) - FILETIME_UNIX_DIFF_SECS
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn filetime_epoch() {
            assert_eq!(filetime_to_unix(116_444_736_000_000_000), 0);
            assert_eq!(filetime_to_unix(116_444_736_000_000_000 + 10_000_000), 1);
        }

        #[test]
        fn long_paths_get_verbatim_prefix() {
            let long = format!(r"C:\{}", "a".repeat(300));
            let w = to_wide(Path::new(&long));
            let s = String::from_utf16_lossy(&w[..w.len() - 1]);
            assert!(s.starts_with(r"\\?\C:\aaa"));
            let short = to_wide(Path::new(r"C:\x"));
            assert_eq!(String::from_utf16_lossy(&short[..short.len() - 1]), r"C:\x");
        }

        #[test]
        fn parses_mount_point_substitute_name() {
            let name: Vec<u8> = r"\??\Volume{1234}\"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect();
            let mut buf = vec![0u8; 16];
            buf[0..4].copy_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
            buf[10..12].copy_from_slice(&(name.len() as u16).to_le_bytes());
            buf.extend_from_slice(&name);
            assert_eq!(substitute_name(&buf).as_deref(), Some(r"\??\Volume{1234}\"));
            assert_eq!(substitute_name(&buf[..12]), None);
        }

        #[test]
        fn name_surrogate_tags_are_links() {
            assert_eq!(
                link_kind(IO_REPARSE_TAG_SYMLINK, || false),
                LinkKind::Symlink
            );
            assert_eq!(
                link_kind(IO_REPARSE_TAG_MOUNT_POINT, || false),
                LinkKind::Junction
            );
            assert_eq!(
                link_kind(IO_REPARSE_TAG_MOUNT_POINT, || true),
                LinkKind::MountPoint
            );
            // OneDrive cloud placeholder tag: a regular file, not a link.
            assert_eq!(link_kind(0x9000_001A, || false), LinkKind::None);
            assert_eq!(link_kind(IO_REPARSE_TAG_WOF, || false), LinkKind::None);
        }

        #[test]
        fn listing_matches_per_file_probe() {
            let tmp = tempfile::tempdir().expect("tempdir");
            std::fs::write(tmp.path().join("f.bin"), vec![1u8; 70_000]).expect("write");
            std::fs::create_dir(tmp.path().join("sub")).expect("mkdir");
            let items = read_dir(tmp.path(), false).expect("read_dir");
            assert_eq!(items.len(), 2);
            for item in items {
                let listed = item.info.expect("info");
                let probed = file_info(&tmp.path().join(&item.name)).expect("probe");
                assert_eq!(listed.is_dir, probed.is_dir);
                assert_eq!(listed.logical, probed.logical);
                assert_eq!(listed.on_disk, probed.on_disk);
                assert_eq!(listed.file_id, probed.file_id, "same 128-bit ID space");
                assert_eq!(listed.volume_id, probed.volume_id);
                assert_eq!(listed.link_count, None);
                assert_eq!(probed.link_count, Some(1));
            }
        }
    }
}

#[cfg(unix)]
mod imp {
    use std::fs;
    use std::io;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    use super::{DirItem, FileFlags, FileInfo, LinkKind, read_dir_probing};

    pub(super) fn file_info(path: &Path) -> io::Result<FileInfo> {
        let md = fs::symlink_metadata(path)?;
        let ft = md.file_type();
        let is_link = ft.is_symlink();
        let is_dir = ft.is_dir();
        let sized = !is_dir && !is_link;
        let mut flags = FileFlags::default();
        if path
            .file_name()
            .is_some_and(|n| n.as_encoded_bytes().starts_with(b"."))
        {
            flags = flags.with(FileFlags::HIDDEN);
        }
        if md.permissions().readonly() {
            flags = flags.with(FileFlags::READONLY);
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::macos::fs::MetadataExt as _;
            flags = flags.with(macos::flags(md.st_flags()));
        }
        let mut link = if is_link {
            LinkKind::Symlink
        } else {
            LinkKind::None
        };
        #[cfg(target_os = "macos")]
        if is_dir && macos::is_data_volume(path) {
            link = LinkKind::MountPoint;
        }
        Ok(FileInfo {
            logical: if sized { md.len() } else { 0 },
            on_disk: if sized {
                md.blocks().saturating_mul(512)
            } else {
                0
            },
            mtime: md.mtime(),
            flags,
            is_dir,
            link,
            link_count: Some(u32::try_from(md.nlink()).unwrap_or(u32::MAX).max(1)),
            volume_id: volume_id(md.dev()),
            file_id: Some(file_id(md.dev(), md.ino())),
            approximate: false,
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn volume_id(dev: u64) -> u64 {
        dev
    }

    #[cfg(not(target_os = "macos"))]
    fn file_id(_dev: u64, ino: u64) -> u128 {
        u128::from(ino)
    }

    /// The Data volume reports the system volume's id (one volume for the walk)…
    #[cfg(target_os = "macos")]
    fn volume_id(dev: u64) -> u64 {
        macos::volume_group(dev)
    }

    /// …so the real device goes into the file id: inode numbers of the two APFS
    /// volumes may collide.
    #[cfg(target_os = "macos")]
    fn file_id(dev: u64, ino: u64) -> u128 {
        (u128::from(dev) << 64) | u128::from(ino)
    }

    pub(super) fn read_dir(dir: &Path, _exact_links: bool) -> io::Result<Vec<DirItem>> {
        read_dir_probing(dir)
    }

    #[cfg(target_os = "macos")]
    pub(super) mod macos {
        use std::os::unix::fs::MetadataExt;
        use std::path::Path;
        use std::sync::OnceLock;

        use super::super::FileFlags;

        // <sys/stat.h> st_flags
        const UF_IMMUTABLE: u32 = 0x0000_0002;
        const UF_COMPRESSED: u32 = 0x0000_0020;
        const UF_HIDDEN: u32 = 0x0000_8000;
        const SF_IMMUTABLE: u32 = 0x0002_0000;
        /// System Integrity Protection.
        const SF_RESTRICTED: u32 = 0x0008_0000;
        /// The contents are in the cloud (iCloud Drive, File Provider): any read
        /// downloads them.
        pub(crate) const SF_DATALESS: u32 = 0x4000_0000;

        /// Where the APFS Data volume of the system volume group is mounted.
        pub(crate) const DATA_VOLUME: &str = "/System/Volumes/Data";

        pub(crate) fn flags(st_flags: u32) -> FileFlags {
            let mut f = FileFlags::default();
            for (bit, flag) in [
                (UF_HIDDEN, FileFlags::HIDDEN),
                (UF_COMPRESSED, FileFlags::COMPRESSED),
                (UF_IMMUTABLE, FileFlags::READONLY),
                (SF_IMMUTABLE, FileFlags::READONLY),
                (SF_RESTRICTED, FileFlags::SYSTEM),
                (SF_DATALESS, FileFlags::OFFLINE),
            ] {
                if st_flags & bit != 0 {
                    f = f.with(flag);
                }
            }
            f
        }

        /// `(data volume device, system volume device)` when they differ
        /// (macOS 10.15+).
        fn group() -> Option<(u64, u64)> {
            static GROUP: OnceLock<Option<(u64, u64)>> = OnceLock::new();
            *GROUP.get_or_init(|| {
                let system = std::fs::symlink_metadata("/").ok()?.dev();
                let data = std::fs::symlink_metadata(DATA_VOLUME).ok()?.dev();
                (system != data).then_some((data, system))
            })
        }

        pub(crate) fn volume_group(dev: u64) -> u64 {
            match group() {
                Some((data, system)) if dev == data => system,
                _ => dev,
            }
        }

        pub(crate) fn is_data_volume(path: &Path) -> bool {
            group().is_some() && path == Path::new(DATA_VOLUME)
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn st_flags_map_onto_file_flags() {
                assert_eq!(flags(0), FileFlags::default());
                let f = flags(SF_DATALESS | UF_HIDDEN);
                assert!(f.contains(FileFlags::OFFLINE) && f.contains(FileFlags::HIDDEN));
                assert!(flags(SF_RESTRICTED).contains(FileFlags::SYSTEM));
                assert!(flags(UF_IMMUTABLE).contains(FileFlags::READONLY));
            }

            #[test]
            fn the_data_volume_is_walked_once() {
                if group().is_some() {
                    let users = super::super::file_info(Path::new("/Users")).expect("/Users");
                    let root = super::super::file_info(Path::new("/")).expect("/");
                    assert_eq!(users.volume_id, root.volume_id, "firmlinked /Users");
                    let data = super::super::file_info(Path::new(DATA_VOLUME)).expect("data");
                    assert_eq!(data.link, super::super::LinkKind::MountPoint);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_file_size_and_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path();
        let file = dir.join("a.bin");
        std::fs::write(&file, vec![7u8; 10_000]).expect("write");

        let f = file_info(&file).expect("file info");
        assert_eq!(f.logical, 10_000);
        assert!(!f.is_dir);
        assert_eq!(f.link, LinkKind::None);
        assert_eq!(f.link_count, Some(1));
        assert!(f.on_disk > 0, "a 10 kB non-resident file occupies clusters");

        let d = file_info(dir).expect("dir info");
        assert!(d.is_dir);
        assert_eq!(d.logical, 0);
    }

    #[test]
    fn read_dir_lists_children_with_metadata() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("one"), b"12345").expect("write");
        std::fs::create_dir(tmp.path().join("two")).expect("mkdir");
        let mut items = read_dir(tmp.path(), true).expect("read_dir");
        items.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<_> = items
            .iter()
            .map(|i| i.name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["one", "two"]);
        let one = items[0].info.as_ref().expect("info");
        assert_eq!(
            (one.logical, one.is_dir, one.link_count),
            (5, false, Some(1))
        );
        assert!(items[1].info.as_ref().expect("info").is_dir);
    }

    #[test]
    fn missing_path_is_an_error() {
        assert!(file_info(Path::new("this/does/not/exist/nfd")).is_err());
        assert!(read_dir(Path::new("this/does/not/exist/nfd"), false).is_err());
    }
}
