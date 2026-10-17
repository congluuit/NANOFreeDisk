//! Chromium's "blockfile" disk cache (`Cache/Cache_Data` on Windows): `index` plus
//! `data_0` (rankings), `data_1` (entries, 256-byte blocks), `data_2` / `data_3`
//! (small data streams) and `f_xxxxxx` (large streams). Format: Chromium
//! `net/disk_cache/blockfile/disk_format.h` and `addr.h`.
//!
//! **Read-only.** Entries share the index's hash chains, the rankings lists and the
//! block files' allocation bitmaps, each protected by a self-hash; removing one entry
//! means rewriting all of them consistently, and a mistake makes the browser throw
//! the whole cache away. So this module only lists what each site holds — a site's
//! entries here are cleaned together with the whole HTTP cache (the browser's rule
//! in the Clean view), never one by one.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// `IndexHeader::magic`.
pub const INDEX_MAGIC: u32 = 0xC103_CAC3;
/// `BlockFileHeader::magic`.
pub const BLOCK_MAGIC: u32 = 0xC104_CAC3;
/// `sizeof(IndexHeader)`.
pub const INDEX_HEADER: usize = 368;
/// `kBlockHeaderSize`.
pub const BLOCK_HEADER: u64 = 8192;
/// `kIndexTablesize` (used when `table_len` is 0).
pub const DEFAULT_TABLE: usize = 0x10000;
/// Offset of `EntryStore::key`.
pub const KEY_OFFSET: usize = 96;
/// `sizeof(EntryStore)`.
pub const ENTRY_SIZE: usize = 256;
/// Index versions this reader knows (2.0, 2.1, 3.0).
pub const VERSIONS: [u32; 3] = [0x20000, 0x20001, 0x30000];

/// One live entry: its key and the bytes of its data streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub bytes: u64,
}

/// What reading a blockfile cache found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// Chains cut short (bad address, loop, unreadable block): the listing is partial.
    pub damaged: u32,
}

/// `true` when `dir` looks like a blockfile cache (`index` with the blockfile magic).
#[must_use]
pub fn is_blockfile(dir: &Path) -> bool {
    let mut buf = [0u8; 4];
    fs::File::open(dir.join("index"))
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_ok_and(|()| u32::from_le_bytes(buf) == INDEX_MAGIC)
}

/// A decoded `CacheAddr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Addr {
    /// `f_%06x`.
    External(u32),
    Block {
        file_type: u32,
        file: u32,
        start: u32,
        blocks: u32,
    },
}

impl Addr {
    /// `None` for an unset (0) or uninitialized address.
    #[must_use]
    pub const fn decode(raw: u32) -> Option<Self> {
        if raw & 0x8000_0000 == 0 {
            return None;
        }
        let file_type = (raw & 0x7000_0000) >> 28;
        if file_type == 0 {
            return Some(Self::External(raw & 0x0FFF_FFFF));
        }
        Some(Self::Block {
            file_type,
            file: (raw & 0x00FF_0000) >> 16,
            start: raw & 0x0000_FFFF,
            blocks: ((raw & 0x0300_0000) >> 24) + 1,
        })
    }

    /// Block size of a block file type (`RANKINGS` 36, `BLOCK_256`, `BLOCK_1K`,
    /// `BLOCK_4K`, `BLOCK_FILES` 8, `BLOCK_ENTRIES` 104, `BLOCK_EVICTED` 48).
    #[must_use]
    pub const fn block_size(file_type: u32) -> Option<u64> {
        match file_type {
            1 => Some(36),
            2 => Some(256),
            3 => Some(1024),
            4 => Some(4096),
            5 => Some(8),
            6 => Some(104),
            7 => Some(48),
            _ => None,
        }
    }

    /// Encode (tests and fixtures).
    #[must_use]
    pub const fn encode(self) -> u32 {
        match self {
            Self::External(n) => 0x8000_0000 | (n & 0x0FFF_FFFF),
            Self::Block {
                file_type,
                file,
                start,
                blocks,
            } => {
                0x8000_0000
                    | ((file_type & 7) << 28)
                    | (((blocks - 1) & 3) << 24)
                    | ((file & 0xFF) << 16)
                    | (start & 0xFFFF)
            }
        }
    }
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn i32_at(b: &[u8], off: usize) -> Option<i32> {
    u32_at(b, off).map(|v| v as i32)
}

struct Reader {
    dir: PathBuf,
    files: Vec<Option<fs::File>>,
}

impl Reader {
    fn block_file(&mut self, n: u32) -> Option<&mut fs::File> {
        let n = n as usize;
        if n >= 256 {
            return None;
        }
        if self.files.len() <= n {
            self.files.resize_with(n + 1, || None);
        }
        if self.files[n].is_none() {
            let mut f = fs::File::open(self.dir.join(format!("data_{n}"))).ok()?;
            let mut head = [0u8; 24];
            f.read_exact(&mut head).ok()?;
            if u32::from_le_bytes([head[0], head[1], head[2], head[3]]) != BLOCK_MAGIC {
                return None;
            }
            self.files[n] = Some(f);
        }
        self.files[n].as_mut()
    }

    /// Bytes at `addr` (`len` capped by the blocks it spans).
    fn read(&mut self, addr: Addr, len: usize) -> Option<Vec<u8>> {
        match addr {
            Addr::External(n) => {
                let mut f = fs::File::open(self.dir.join(format!("f_{n:06x}"))).ok()?;
                let mut buf = vec![0u8; len];
                f.read_exact(&mut buf).ok()?;
                Some(buf)
            }
            Addr::Block {
                file_type,
                file,
                start,
                blocks,
            } => {
                let size = Addr::block_size(file_type)?;
                let span = (size * u64::from(blocks)) as usize;
                let f = self.block_file(file)?;
                let mut head = [0u8; 24];
                f.seek(SeekFrom::Start(0)).ok()?;
                f.read_exact(&mut head).ok()?;
                let entry_size = i32_at(&head, 12)?;
                let max_entries = i32_at(&head, 20)?;
                if i64::from(entry_size) != size as i64
                    || i64::from(start) >= i64::from(max_entries)
                {
                    return None;
                }
                let mut buf = vec![0u8; len.min(span)];
                f.seek(SeekFrom::Start(BLOCK_HEADER + u64::from(start) * size))
                    .ok()?;
                f.read_exact(&mut buf).ok()?;
                Some(buf)
            }
        }
    }
}

/// Every live entry of the blockfile cache in `dir`, read-only. Stops after
/// `max_entries` entries.
///
/// # Errors
/// The index cannot be read or is not a blockfile index this reader knows.
pub fn list(dir: &Path, max_entries: usize) -> std::io::Result<Listing> {
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_owned());
    let index = fs::read(dir.join("index"))?;
    if u32_at(&index, 0) != Some(INDEX_MAGIC) {
        return Err(bad("not a blockfile index"));
    }
    let version = u32_at(&index, 4).unwrap_or(0);
    if !VERSIONS.contains(&version) {
        return Err(bad("unknown blockfile version"));
    }
    let table_len = match i32_at(&index, 28).unwrap_or(0) {
        0 => DEFAULT_TABLE,
        n if n > 0 && (n as usize).is_power_of_two() => n as usize,
        _ => return Err(bad("bad table length")),
    };
    if index.len() < INDEX_HEADER + table_len * 4 {
        return Err(bad("index too short"));
    }
    let mut reader = Reader {
        dir: dir.to_path_buf(),
        files: Vec::new(),
    };
    let mut out = Listing::default();
    let mut seen: HashSet<u32> = HashSet::new();
    for slot in 0..table_len {
        let mut raw = u32_at(&index, INDEX_HEADER + slot * 4).unwrap_or(0);
        let mut hops = 0;
        while raw != 0 {
            hops += 1;
            if hops > 10_000 || !seen.insert(raw) {
                out.damaged += 1;
                break;
            }
            let Some(
                addr @ Addr::Block {
                    file_type: 2,
                    blocks,
                    ..
                },
            ) = Addr::decode(raw)
            else {
                out.damaged += 1;
                break;
            };
            let Some(store) = reader.read(addr, ENTRY_SIZE * blocks as usize) else {
                out.damaged += 1;
                break;
            };
            raw = u32_at(&store, 4).unwrap_or(0);
            let state = i32_at(&store, 20).unwrap_or(-1);
            if state != 0 {
                continue; // evicted or doomed
            }
            let key_len = i32_at(&store, 32).unwrap_or(-1);
            if !(1..=1 << 20).contains(&key_len) {
                continue;
            }
            let key_len = key_len as usize;
            let long_key = u32_at(&store, 36).unwrap_or(0);
            let key = if long_key == 0 {
                store
                    .get(KEY_OFFSET..KEY_OFFSET + key_len)
                    .map(<[u8]>::to_vec)
            } else {
                Addr::decode(long_key).and_then(|a| reader.read(a, key_len))
            };
            let Some(key) = key else {
                out.damaged += 1;
                continue;
            };
            let bytes = (0..4)
                .map(|i| u64::try_from(i32_at(&store, 40 + i * 4).unwrap_or(0)).unwrap_or(0))
                .sum();
            out.entries.push(Entry {
                key: String::from_utf8_lossy(&key).into_owned(),
                bytes,
            });
            if out.entries.len() >= max_entries {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn addresses_round_trip() {
        for a in [
            Addr::External(0x12_3456),
            Addr::Block {
                file_type: 2,
                file: 1,
                start: 77,
                blocks: 4,
            },
            Addr::Block {
                file_type: 1,
                file: 0,
                start: 0,
                blocks: 1,
            },
        ] {
            assert_eq!(Addr::decode(a.encode()), Some(a));
        }
        assert_eq!(Addr::decode(0), None);
        assert_eq!(Addr::decode(0x1234), None);
    }

    #[test]
    fn lists_entries_with_inline_and_long_keys() {
        let dir = tempfile::tempdir().unwrap();
        let long = format!(
            "1/0/_dk_https://long.example https://long.example https://cdn.example/{}",
            "x".repeat(1200)
        );
        fixture::blockfile(
            dir.path(),
            &[
                (
                    "1/0/_dk_https://youtube.com https://youtube.com https://i.ytimg.com/a.jpg",
                    1000,
                ),
                (
                    "1/0/_dk_https://youtube.com https://youtube.com https://i.ytimg.com/b.jpg",
                    2000,
                ),
                ("https://example.com/style.css", 300),
                (long.as_str(), 50),
            ],
        );
        assert!(is_blockfile(dir.path()));
        let l = list(dir.path(), 100).unwrap();
        assert_eq!(l.damaged, 0);
        assert_eq!(l.entries.len(), 4);
        let total: u64 = l.entries.iter().map(|e| e.bytes).sum();
        assert_eq!(total, 3350);
        assert!(l.entries.iter().any(|e| e.key == long));
    }

    #[test]
    fn refuses_garbage_and_survives_loops() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("index"), b"not an index").unwrap();
        assert!(!is_blockfile(dir.path()));
        assert!(list(dir.path(), 10).is_err());

        // an entry whose `next` points to itself is read once, the loop counted
        let dir = tempfile::tempdir().unwrap();
        fixture::blockfile(dir.path(), &[("https://loop.example/", 10)]);
        let mut data1 = fs::read(dir.path().join("data_1")).unwrap();
        let me = Addr::Block {
            file_type: 2,
            file: 1,
            start: 0,
            blocks: 1,
        }
        .encode();
        data1[BLOCK_HEADER as usize + 4..BLOCK_HEADER as usize + 8]
            .copy_from_slice(&me.to_le_bytes());
        fs::write(dir.path().join("data_1"), data1).unwrap();
        let l = list(dir.path(), 10).unwrap();
        assert_eq!(l.entries.len(), 1);
        assert_eq!(l.damaged, 1);
    }

    #[test]
    fn listing_never_writes() {
        let dir = tempfile::tempdir().unwrap();
        fixture::blockfile(dir.path(), &[("https://a.example/", 1)]);
        let before: Vec<_> = ["index", "data_0", "data_1", "data_2", "data_3"]
            .iter()
            .map(|n| fs::read(dir.path().join(n)).unwrap())
            .collect();
        list(dir.path(), 10).unwrap();
        let after: Vec<_> = ["index", "data_0", "data_1", "data_2", "data_3"]
            .iter()
            .map(|n| fs::read(dir.path().join(n)).unwrap())
            .collect();
        assert_eq!(before, after);
    }
}
