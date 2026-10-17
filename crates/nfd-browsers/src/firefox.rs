//! Firefox `cache2`: `entries/<SHA-1 of the key>` holds the data, then the
//! metadata, and its last 4 bytes (big endian) give where the metadata starts.
//! Metadata: a 4-byte hash, one 2-byte hash per 256 KiB chunk of data, the header
//! (`version`, fetch count, last fetched, last modified, frecency, expiration, key
//! size, and from version 2 on `flags`; all big endian), then the key and a NUL.
//! Format: Firefox `netwerk/cache2/CacheFileMetadata.h`.
//!
//! Removing an entry = removing its file. The `cache2/index` is removed with it:
//! Firefox rebuilds a missing index from the entry files on the next start, so the
//! index never points at an entry that is gone.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rayon::prelude::*;

/// `kChunkSize`.
pub const CHUNK: u64 = 256 * 1024;

/// One entry file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub key: Option<String>,
    pub bytes: u64,
}

fn be32(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

/// The key of a cache2 entry file, or `None` when it is not one.
#[must_use]
pub fn read_key(path: &Path) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len < 4 + 4 + 32 {
        return None;
    }
    f.seek(SeekFrom::Start(len - 4)).ok()?;
    let mut tail = [0u8; 4];
    f.read_exact(&mut tail).ok()?;
    let meta = u64::from(u32::from_be_bytes(tail));
    if meta >= len - 4 {
        return None;
    }
    let chunks = meta.div_ceil(CHUNK);
    let head_at = meta + 4 + chunks * 2;
    let avail = (len - 4).checked_sub(head_at)?;
    let mut rest = vec![0u8; usize::try_from(avail.min(64 * 1024 + 64)).ok()?];
    f.seek(SeekFrom::Start(head_at)).ok()?;
    f.read_exact(&mut rest).ok()?;
    let version = be32(&rest, 0)?;
    let key_size = be32(&rest, 24)? as usize;
    // version 1 has no flags field
    let key_at = if version >= 2 { 32 } else { 28 };
    if !(1..=4).contains(&version) || key_size == 0 {
        return None;
    }
    let key = rest.get(key_at..key_at + key_size)?;
    if rest.get(key_at + key_size) != Some(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(key).into_owned())
}

/// `true` for a cache2 entry file name (40 upper-case hex digits).
#[must_use]
pub fn entry_name(name: &str) -> bool {
    name.len() == 40 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Every entry of `cache2/entries` (at most `max`). Links are skipped.
#[must_use]
pub fn list(entries_dir: &Path, max: usize) -> Vec<Entry> {
    let Ok(rd) = fs::read_dir(entries_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        if out.len() >= max {
            break;
        }
        if !entry_name(&e.file_name().to_string_lossy()) {
            continue;
        }
        let Ok(m) = fs::symlink_metadata(e.path()) else {
            continue;
        };
        if !m.is_file() {
            continue;
        }
        out.push(Entry {
            key: None,
            path: e.path(),
            bytes: m.len(),
        });
    }
    out.par_iter_mut().for_each(|e| e.key = read_key(&e.path));
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn reads_keys_of_both_versions() {
        let dir = tempfile::tempdir().unwrap();
        let a = fixture::firefox_entry(dir.path(), "a,:https://www.mozilla.org/", 300_000, 3);
        let b = fixture::firefox_entry(dir.path(), ":http://example.com/", 10, 1);
        assert_eq!(read_key(&a).as_deref(), Some("a,:https://www.mozilla.org/"));
        assert_eq!(read_key(&b).as_deref(), Some(":http://example.com/"));
        let l = list(dir.path(), 10);
        assert_eq!(l.len(), 2);
        assert!(l.iter().all(|e| e.key.is_some()));
    }

    #[test]
    fn garbage_has_no_key() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("0123456789ABCDEF0123456789ABCDEF01234567");
        fs::write(&p, vec![0xffu8; 100]).unwrap();
        assert_eq!(read_key(&p), None);
        fs::write(&p, b"tiny").unwrap();
        assert_eq!(read_key(&p), None);
    }
}
