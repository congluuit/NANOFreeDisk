//! Chromium's "Simple" disk cache: one file per entry, `<16 hex>_0` (streams 0 and
//! 1), optionally `<hash>_1` (stream 2) and `<hash>_s` (sparse data), plus the
//! advisory index `index-dir/the-real-index`. Used by `Code Cache/js`,
//! `Code Cache/wasm`, every CacheStorage cache and, where the browser chose it, the
//! HTTP cache. Format: Chromium `net/disk_cache/simple/simple_entry_format.h`.
//!
//! Removing an entry = removing its files. The backend is built for that: the
//! index is only a hint, a missing entry is a cache miss, and an index older than
//! its folder (or missing) is rebuilt from the files on the next start. Removing
//! `the-real-index` together with the entries makes that rebuild certain.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

/// `kSimpleInitialMagicNumber`.
pub const INITIAL_MAGIC: u64 = 0xfcfb_6d1b_a772_5c30;
/// `sizeof(SimpleFileHeader)`: magic, version, key length, key hash, padding.
pub const HEADER: usize = 24;
/// Longest key read (Chromium caps keys far below this).
pub const MAX_KEY: usize = 64 * 1024;

/// One entry: its files (all of them go together), key and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `<hash>_0`, `<hash>_1`, `<hash>_s` that exist.
    pub files: Vec<PathBuf>,
    /// `None` when no file had a readable header.
    pub key: Option<String>,
    pub bytes: u64,
}

/// `<16 hex>_0|_1|_s` → (hash, suffix).
#[must_use]
pub fn entry_file(name: &str) -> Option<(&str, char)> {
    let (hash, suffix) = name.split_once('_')?;
    if hash.len() != 16 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match suffix {
        "0" => Some((hash, '0')),
        "1" => Some((hash, '1')),
        "s" => Some((hash, 's')),
        _ => None,
    }
}

/// The key stored in an entry file's header, or `None` when it is not one.
#[must_use]
pub fn read_key(path: &Path) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    let mut head = [0u8; HEADER];
    f.read_exact(&mut head).ok()?;
    let magic = u64::from_le_bytes(head[0..8].try_into().ok()?);
    if magic != INITIAL_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(head[8..12].try_into().ok()?);
    let key_len = u32::from_le_bytes(head[12..16].try_into().ok()?) as usize;
    if !(1..=64).contains(&version) || key_len == 0 || key_len > MAX_KEY {
        return None;
    }
    let mut key = vec![0u8; key_len];
    f.read_exact(&mut key).ok()?;
    Some(String::from_utf8_lossy(&key).into_owned())
}

/// `true` when `dir` holds Simple cache entries (or its index folder).
#[must_use]
pub fn is_simple(dir: &Path) -> bool {
    if dir.join("index-dir").is_dir() {
        return true;
    }
    fs::read_dir(dir).is_ok_and(|rd| {
        rd.flatten()
            .take(64)
            .any(|e| entry_file(&e.file_name().to_string_lossy()).is_some())
    })
}

/// Every entry in `dir` (not recursive), at most `max` of them. Links are skipped.
#[must_use]
pub fn list(dir: &Path, max: usize) -> Vec<Entry> {
    let mut by_hash: BTreeMap<String, Entry> = BTreeMap::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some((hash, _)) = entry_file(&name) else {
            continue;
        };
        let Ok(meta) = fs::symlink_metadata(e.path()) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if by_hash.len() >= max && !by_hash.contains_key(hash) {
            continue;
        }
        let entry = by_hash.entry(hash.to_owned()).or_insert_with(|| Entry {
            files: Vec::new(),
            key: None,
            bytes: 0,
        });
        entry.files.push(e.path());
        entry.bytes += meta.len();
    }
    let mut out: Vec<Entry> = by_hash.into_values().collect();
    out.par_iter_mut().for_each(|entry| {
        entry.files.sort();
        // `_0` sorts first and holds the key; the others repeat it
        entry.key = entry.files.iter().find_map(|f| read_key(f));
    });
    out
}

/// The advisory index of a Simple cache folder, when present.
#[must_use]
pub fn index_file(dir: &Path) -> Option<PathBuf> {
    let p = dir.join("index-dir").join("the-real-index");
    fs::symlink_metadata(&p)
        .is_ok_and(|m| m.is_file())
        .then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn names() {
        assert_eq!(
            entry_file("0000801decaa5caa_0"),
            Some(("0000801decaa5caa", '0'))
        );
        assert_eq!(
            entry_file("0000801decaa5caa_s"),
            Some(("0000801decaa5caa", 's'))
        );
        assert_eq!(entry_file("0000801decaa5caa_2"), None);
        assert_eq!(entry_file("index"), None);
        assert_eq!(entry_file("zz00801decaa5caa_0"), None);
    }

    #[test]
    fn reads_keys_and_groups_files() {
        let dir = tempfile::tempdir().unwrap();
        fixture::simple_entry(
            dir.path(),
            "_keyhttps://a.example/x.js \nhttps://a.example/",
            100,
            true,
        );
        fixture::simple_entry(
            dir.path(),
            "_keyhttps://b.example/y.js \nhttps://b.example/",
            50,
            false,
        );
        fs::write(dir.path().join("index"), b"fake index").unwrap();
        fixture::simple_index(dir.path());
        assert!(is_simple(dir.path()));
        let entries = list(dir.path(), 100);
        assert_eq!(entries.len(), 2);
        let a = entries
            .iter()
            .find(|e| e.key.as_deref().is_some_and(|k| k.contains("a.example")))
            .unwrap();
        assert_eq!(a.files.len(), 2, "{a:?}");
        assert!(a.bytes > 100);
        assert!(index_file(dir.path()).is_some());
    }

    #[test]
    fn garbage_has_no_key() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("0123456789abcdef_0");
        fs::write(&p, b"definitely not a cache entry").unwrap();
        assert_eq!(read_key(&p), None);
        assert_eq!(list(dir.path(), 10)[0].key, None);
    }
}
