//! Chromium CacheStorage (`Service Worker/CacheStorage`): one folder per origin
//! (named by a hash of it) holding `index.txt` — a `CacheStorageIndex` protobuf
//! naming the origin, its storage key and the caches — and one Simple cache folder
//! per cache. A whole origin folder is what one site's Service Worker caches; the
//! browser treats a missing folder as an origin with no caches.

use std::fs;
use std::path::{Path, PathBuf};

/// One origin folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginDir {
    pub path: PathBuf,
    pub origin: Option<String>,
    pub storage_key: Option<String>,
    pub bytes: u64,
    pub files: u64,
}

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*i)?;
        *i += 1;
        v |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// Top-level string fields 2 (origin) and 3 (storage key) of a
/// `CacheStorageIndex`; other fields are skipped by wire type.
#[must_use]
pub fn parse_index(b: &[u8]) -> (Option<String>, Option<String>) {
    let mut origin = None;
    let mut storage_key = None;
    let mut i = 0;
    while i < b.len() {
        let Some(tag) = varint(b, &mut i) else { break };
        let (field, wire) = (tag >> 3, tag & 7);
        match wire {
            0 => {
                if varint(b, &mut i).is_none() {
                    break;
                }
            }
            1 => i += 8,
            5 => i += 4,
            2 => {
                let Some(len) = varint(b, &mut i) else { break };
                let Ok(len) = usize::try_from(len) else { break };
                let Some(bytes) = b.get(i..i.saturating_add(len)) else {
                    break;
                };
                i += len;
                let text = || std::str::from_utf8(bytes).ok().map(str::to_owned);
                match field {
                    2 => origin = text(),
                    3 => storage_key = text(),
                    _ => {}
                }
            }
            _ => break,
        }
    }
    (origin, storage_key)
}

/// Size and file count of a folder tree (links not followed, at most `max` entries).
#[must_use]
pub fn tree_size(dir: &Path, max: u64) -> (u64, u64) {
    let mut bytes = 0;
    let mut files = 0;
    let mut seen = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > max {
                return (bytes, files);
            }
            let Ok(m) = fs::symlink_metadata(e.path()) else {
                continue;
            };
            if m.is_dir() {
                stack.push(e.path());
            } else if m.is_file() {
                bytes += m.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

/// Every origin folder under `root` (`…/Service Worker/CacheStorage`).
#[must_use]
pub fn list(root: &Path, max_entries: u64) -> Vec<OriginDir> {
    let Ok(rd) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let Ok(m) = fs::symlink_metadata(e.path()) else {
            continue;
        };
        if !m.is_dir() {
            continue;
        }
        let (origin, storage_key) = fs::read(e.path().join("index.txt"))
            .map(|b| parse_index(&b))
            .unwrap_or((None, None));
        let (bytes, files) = tree_size(&e.path(), max_entries);
        out.push(OriginDir {
            path: e.path(),
            origin,
            storage_key,
            bytes,
            files,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn parses_the_real_layout() {
        // field 1 (a cache), then origin and storage key, then two varints
        let mut b = Vec::new();
        b.extend([0x0a, 0x03, 0x0a, 0x01, b'x']);
        b.extend([0x12, 0x18]);
        b.extend(b"https://www.youtube.com/");
        b.extend([0x1a, 0x18]);
        b.extend(b"https://www.youtube.com/");
        b.extend([0x20, 0x03, 0x28, 0x01]);
        let (o, k) = parse_index(&b);
        assert_eq!(o.as_deref(), Some("https://www.youtube.com/"));
        assert_eq!(k.as_deref(), Some("https://www.youtube.com/"));
        assert_eq!(parse_index(&[0xff, 0xff]), (None, None));
    }

    #[test]
    fn lists_origins() {
        let dir = tempfile::tempdir().unwrap();
        fixture::cache_storage_origin(dir.path(), "aaaa", "https://www.youtube.com/", 4000);
        fixture::cache_storage_origin(dir.path(), "bbbb", "https://mail.example/", 10);
        let l = list(dir.path(), 10_000);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].origin.as_deref(), Some("https://www.youtube.com/"));
        assert!(l[0].bytes > 4000 && l[0].files >= 2);
    }
}
