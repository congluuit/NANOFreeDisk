//! Reading file contents: the [`Source`] the pipeline reads through, the partial
//! and full BLAKE3 digests, and the byte-by-byte comparison of the paranoid mode.
//!
//! Every read goes through [`Source::info`] first, which looks at the file **without
//! opening its data** (`FILE_READ_ATTRIBUTES`): a cloud placeholder (OneDrive "online
//! only", `RECALL_ON_OPEN` / `RECALL_ON_DATA_ACCESS`) or a link is refused before
//! [`Source::open`] is ever called, so the sync engine never downloads anything. A
//! file whose size or last write differ from what the scan saw is refused too, and
//! checked again after hashing: a file rewritten while it was read never joins a
//! group.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::SkipReason;

/// Bytes read at the head, the middle and the tail for the partial hash.
pub const SAMPLE: u64 = 16 * 1024;
/// Read buffer of the full hash and the comparison (no memory mapping: a file
/// truncated by another program while mapped would crash the reader).
pub const BUFFER: usize = 1 << 20;

/// What the pipeline needs to know about a file before (and after) reading it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LiveInfo {
    pub size: u64,
    /// Last write, Unix seconds.
    pub mtime: i64,
    /// Cloud placeholder / offline data: must never be opened for reading.
    pub cloud: bool,
    /// Symlink, junction or mount point.
    pub link: bool,
    pub is_dir: bool,
    pub volume: u64,
    pub file_id: Option<u128>,
    pub link_count: Option<u32>,
}

/// A readable, seekable file.
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// Where contents come from: the real disk ([`LiveSource`]) or a fake in tests.
pub trait Source: Sync {
    /// Metadata without touching the data (links not followed).
    ///
    /// # Errors
    /// The file is gone or cannot be queried.
    fn info(&self, path: &Path) -> io::Result<LiveInfo>;
    /// Open the data for one sequential pass.
    ///
    /// # Errors
    /// The file cannot be opened.
    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadSeek>>;
    /// `Some(true)` for a spinning disk (read one file at a time).
    fn seek_penalty(&self, path: &Path) -> Option<bool>;
    /// How a duplicate on the volume of `path` can be replaced by a link.
    fn link_mode(&self, _path: &Path) -> Option<crate::LinkKind> {
        None
    }
}

/// The real filesystem (`nfd_platform::fsinfo` + `nfd_platform::io`).
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveSource;

impl Source for LiveSource {
    fn info(&self, path: &Path) -> io::Result<LiveInfo> {
        use nfd_platform::fsinfo::{FileFlags, LinkKind, file_info};
        let i = file_info(path)?;
        Ok(LiveInfo {
            size: i.logical,
            mtime: i.mtime,
            cloud: i.flags.contains(FileFlags::OFFLINE),
            link: i.link != LinkKind::None,
            is_dir: i.is_dir,
            volume: i.volume_id,
            file_id: i.file_id,
            link_count: i.link_count,
        })
    }

    fn open(&self, path: &Path) -> io::Result<Box<dyn ReadSeek>> {
        Ok(Box::new(nfd_platform::io::open_sequential(path)?))
    }

    fn seek_penalty(&self, path: &Path) -> Option<bool> {
        nfd_platform::volumes::seek_penalty(path)
    }

    fn link_mode(&self, path: &Path) -> Option<crate::LinkKind> {
        nfd_platform::link::mode(path).map(|m| match m {
            nfd_platform::link::LinkMode::Clone => crate::LinkKind::Clone,
            nfd_platform::link::LinkMode::Hardlink => crate::LinkKind::Hardlink,
        })
    }
}

/// Shared state of one run: cancellation and the byte counter for progress.
#[derive(Debug, Default)]
pub struct Ctx<'a> {
    pub cancel: Option<&'a AtomicBool>,
    pub bytes: AtomicU64,
}

impl Ctx<'_> {
    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
    }
}

/// Why a file could not be hashed (it simply leaves the pipeline).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Skip(SkipReason),
    Cancelled,
}

impl From<io::Error> for Refused {
    fn from(_: io::Error) -> Self {
        Self::Skip(SkipReason::Unreadable)
    }
}

/// Refuse anything that is not the plain local file the scan saw.
pub(crate) fn check(
    source: &dyn Source,
    path: &Path,
    size: u64,
    mtime: i64,
) -> Result<LiveInfo, Refused> {
    let info = source.info(path)?;
    if info.cloud {
        return Err(Refused::Skip(SkipReason::Cloud));
    }
    if info.link || info.is_dir {
        return Err(Refused::Skip(SkipReason::Reparse));
    }
    if info.size != size || info.mtime != mtime {
        return Err(Refused::Skip(SkipReason::Changed));
    }
    Ok(info)
}

/// Partial digest: size + 16 KiB at the head, the middle and the tail. Files of at
/// most 48 KiB are read whole and the digest is also their full hash (`true`).
///
/// # Errors
/// [`Refused`] when the file is a placeholder, changed, unreadable, or on cancel.
pub fn partial(
    source: &dyn Source,
    path: &Path,
    size: u64,
    mtime: i64,
    ctx: &Ctx<'_>,
) -> Result<([u8; 32], bool), Refused> {
    if ctx.cancelled() {
        return Err(Refused::Cancelled);
    }
    check(source, path, size, mtime)?;
    let mut f = source.open(path)?;
    let mut h = blake3::Hasher::new();
    let whole = size <= 3 * SAMPLE;
    if whole {
        let mut buf = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
        f.read_to_end(&mut buf)?;
        if buf.len() as u64 != size {
            return Err(Refused::Skip(SkipReason::Changed));
        }
        h.update(&buf);
    } else {
        h.update(&size.to_le_bytes());
        let mut buf = vec![0u8; SAMPLE as usize];
        for at in [0, size / 2 - SAMPLE / 2, size - SAMPLE] {
            f.seek(SeekFrom::Start(at))?;
            f.read_exact(&mut buf)?;
            h.update(&buf);
        }
    }
    drop(f);
    ctx.bytes.fetch_add(size.min(3 * SAMPLE), Ordering::Relaxed);
    check(source, path, size, mtime)?;
    Ok((*h.finalize().as_bytes(), whole))
}

/// Full BLAKE3 digest of the whole file.
///
/// # Errors
/// [`Refused`] when the file is a placeholder, changed, unreadable, or on cancel.
pub fn full(
    source: &dyn Source,
    path: &Path,
    size: u64,
    mtime: i64,
    ctx: &Ctx<'_>,
) -> Result<[u8; 32], Refused> {
    if ctx.cancelled() {
        return Err(Refused::Cancelled);
    }
    check(source, path, size, mtime)?;
    let mut f = source.open(path)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; BUFFER];
    let mut total = 0u64;
    loop {
        if ctx.cancelled() {
            return Err(Refused::Cancelled);
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        total += n as u64;
        ctx.bytes.fetch_add(n as u64, Ordering::Relaxed);
    }
    drop(f);
    if total != size {
        return Err(Refused::Skip(SkipReason::Changed));
    }
    check(source, path, size, mtime)?;
    Ok(*h.finalize().as_bytes())
}

/// `true` when two files have exactly the same bytes (paranoid mode and the final
/// check before an action).
///
/// # Errors
/// [`Refused`] when either file is a placeholder, changed, unreadable, or on cancel.
pub fn same_bytes(
    source: &dyn Source,
    a: (&Path, u64, i64),
    b: (&Path, u64, i64),
    ctx: &Ctx<'_>,
) -> Result<bool, Refused> {
    if a.1 != b.1 {
        return Ok(false);
    }
    check(source, a.0, a.1, a.2)?;
    check(source, b.0, b.1, b.2)?;
    let mut fa = source.open(a.0)?;
    let mut fb = source.open(b.0)?;
    let mut ba = vec![0u8; BUFFER];
    let mut bb = vec![0u8; BUFFER];
    loop {
        if ctx.cancelled() {
            return Err(Refused::Cancelled);
        }
        let na = read_full(&mut fa, &mut ba)?;
        let nb = read_full(&mut fb, &mut bb)?;
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            break;
        }
        ctx.bytes.fetch_add(2 * na as u64, Ordering::Relaxed);
    }
    drop((fa, fb));
    check(source, a.0, a.1, a.2)?;
    check(source, b.0, b.1, b.2)?;
    Ok(true)
}

/// Fill `buf` as far as the file allows.
fn read_full(f: &mut dyn ReadSeek, buf: &mut [u8]) -> io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match f.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}

/// Lower-case hex of a digest.
#[must_use]
pub fn hex(d: &[u8; 32]) -> String {
    use std::fmt::Write;
    d.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}
