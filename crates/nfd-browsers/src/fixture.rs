//! Synthetic caches for tests (and `nfd browsers` integration tests): small but
//! byte-for-byte in the formats the readers expect. Never used on real profiles.
#![allow(clippy::unwrap_used, clippy::missing_panics_doc)]

use std::fs;
use std::path::{Path, PathBuf};

use crate::blockfile::{self, Addr};

fn fnv(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn block_header(entry_size: i32, this_file: i16, max_entries: i32, used: i32) -> Vec<u8> {
    let mut h = vec![0u8; blockfile::BLOCK_HEADER as usize];
    h[0..4].copy_from_slice(&blockfile::BLOCK_MAGIC.to_le_bytes());
    h[4..8].copy_from_slice(&0x30000u32.to_le_bytes());
    h[8..10].copy_from_slice(&this_file.to_le_bytes());
    h[12..16].copy_from_slice(&entry_size.to_le_bytes());
    h[16..20].copy_from_slice(&used.to_le_bytes());
    h[20..24].copy_from_slice(&max_entries.to_le_bytes());
    h
}

/// A blockfile cache in `dir` with one entry per `(key, data bytes)`.
pub fn blockfile(dir: &Path, entries: &[(&str, i32)]) {
    fs::create_dir_all(dir).unwrap();
    let table_len = blockfile::DEFAULT_TABLE;
    let mut index = vec![0u8; blockfile::INDEX_HEADER + table_len * 4];
    index[0..4].copy_from_slice(&blockfile::INDEX_MAGIC.to_le_bytes());
    index[4..8].copy_from_slice(&0x30000u32.to_le_bytes());
    index[8..12].copy_from_slice(&(entries.len() as i32).to_le_bytes());
    let max = 1024;
    let mut data1 = vec![0u8; max * blockfile::ENTRY_SIZE];
    let mut next_block = 0usize;
    let mut external = 1u32;
    for (n, (key, bytes)) in entries.iter().enumerate() {
        let max_inline = 4 * blockfile::ENTRY_SIZE - blockfile::KEY_OFFSET - 1;
        let (blocks, long_key) = if key.len() <= max_inline {
            (
                (blockfile::KEY_OFFSET + key.len())
                    .div_ceil(blockfile::ENTRY_SIZE)
                    .max(1),
                0,
            )
        } else {
            let addr = Addr::External(external);
            fs::write(dir.join(format!("f_{external:06x}")), key.as_bytes()).unwrap();
            external += 1;
            (1, addr.encode())
        };
        let start = next_block;
        next_block += blocks;
        let addr = Addr::Block {
            file_type: 2,
            file: 1,
            start: start as u32,
            blocks: blocks as u32,
        }
        .encode();
        let hash = fnv(key) as u32;
        let slot = blockfile::INDEX_HEADER + (hash as usize & (table_len - 1)) * 4;
        let previous = u32::from_le_bytes(index[slot..slot + 4].try_into().unwrap());
        index[slot..slot + 4].copy_from_slice(&addr.to_le_bytes());
        let e = &mut data1[start * blockfile::ENTRY_SIZE..(start + blocks) * blockfile::ENTRY_SIZE];
        e[0..4].copy_from_slice(&hash.to_le_bytes());
        e[4..8].copy_from_slice(&previous.to_le_bytes());
        let rank = Addr::Block {
            file_type: 1,
            file: 0,
            start: n as u32,
            blocks: 1,
        };
        e[8..12].copy_from_slice(&rank.encode().to_le_bytes());
        e[32..36].copy_from_slice(&(key.len() as i32).to_le_bytes());
        e[36..40].copy_from_slice(&long_key.to_le_bytes());
        e[44..48].copy_from_slice(&bytes.to_le_bytes()); // data_size[1]: the body
        if long_key == 0 {
            e[blockfile::KEY_OFFSET..blockfile::KEY_OFFSET + key.len()]
                .copy_from_slice(key.as_bytes());
        }
    }
    fs::write(dir.join("index"), index).unwrap();
    let mut f1 = block_header(256, 1, max as i32, next_block as i32);
    f1.extend(data1);
    fs::write(dir.join("data_1"), f1).unwrap();
    let mut f0 = block_header(36, 0, max as i32, entries.len() as i32);
    f0.extend(vec![0u8; max * 36]);
    fs::write(dir.join("data_0"), f0).unwrap();
    fs::write(dir.join("data_2"), block_header(1024, 2, 0, 0)).unwrap();
    fs::write(dir.join("data_3"), block_header(4096, 3, 0, 0)).unwrap();
}

/// A Simple cache entry for `key` with `data` bytes in `dir`; `stream2` also
/// writes `<hash>_1`. Returns the `_0` path.
pub fn simple_entry(dir: &Path, key: &str, data: usize, stream2: bool) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let hash = format!("{:016x}", fnv(key));
    let header = |extra: usize| {
        let mut b = Vec::new();
        b.extend(crate::simple::INITIAL_MAGIC.to_le_bytes());
        b.extend(5u32.to_le_bytes());
        b.extend((key.len() as u32).to_le_bytes());
        b.extend((fnv(key) as u32).to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(key.as_bytes());
        b.extend(vec![0x5a; extra]);
        b.extend(0xf4fa_6f45_970d_41d8u64.to_le_bytes()); // an EOF record's magic
        b.extend([0u8; 16]);
        b
    };
    let p0 = dir.join(format!("{hash}_0"));
    fs::write(&p0, header(data)).unwrap();
    if stream2 {
        fs::write(dir.join(format!("{hash}_1")), header(16)).unwrap();
    }
    p0
}

/// `index-dir/the-real-index` (contents are not read by the cleaner).
pub fn simple_index(dir: &Path) -> PathBuf {
    let p = dir.join("index-dir").join("the-real-index");
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, [0x11u8; 40]).unwrap();
    fs::write(dir.join("index"), [0x30u8; 24]).unwrap();
    p
}

fn proto_str(field: u8, text: &str, out: &mut Vec<u8>) {
    out.push((field << 3) | 2);
    let mut len = text.len();
    loop {
        let b = (len & 0x7f) as u8;
        len >>= 7;
        if len == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
    out.extend(text.as_bytes());
}

/// A CacheStorage origin folder `root/<name>` for `origin` with one cache holding
/// `data` bytes.
pub fn cache_storage_origin(root: &Path, name: &str, origin: &str, data: usize) -> PathBuf {
    let dir = root.join(name);
    let cache = dir.join("4597f45e-7339-4937-90c7-e4a70ea59a7d");
    fs::create_dir_all(&cache).unwrap();
    let mut c = Vec::new();
    proto_str(1, "assets", &mut c);
    proto_str(2, "4597f45e-7339-4937-90c7-e4a70ea59a7d", &mut c);
    let mut idx = Vec::new();
    idx.push(0x0a);
    idx.push(c.len() as u8);
    idx.extend(c);
    proto_str(2, origin, &mut idx);
    proto_str(3, origin, &mut idx);
    idx.extend([0x20, 0x03, 0x28, 0x01]);
    fs::write(dir.join("index.txt"), idx).unwrap();
    simple_entry(&cache, &format!("{origin}app.js"), data, false);
    simple_index(&cache);
    dir
}

/// A Firefox cache2 entry for `key` with `data` bytes (metadata `version` 1–3).
pub fn firefox_entry(entries: &Path, key: &str, data: usize, version: u32) -> PathBuf {
    fs::create_dir_all(entries).unwrap();
    let h = fnv(key);
    let name = format!("{h:016X}{:016X}{:08X}", h.rotate_left(17), (h >> 7) as u32);
    let mut b = vec![0x42u8; data];
    b.extend(0x1234_5678u32.to_be_bytes()); // metadata hash
    let chunks = (data as u64).div_ceil(crate::firefox::CHUNK);
    b.extend(vec![0u8; chunks as usize * 2]);
    for v in [
        version,
        1,
        1_700_000_000,
        1_700_000_000,
        10,
        0xffff_ffff,
        key.len() as u32,
    ] {
        b.extend(v.to_be_bytes());
    }
    if version >= 2 {
        b.extend(0u32.to_be_bytes());
    }
    b.extend(key.as_bytes());
    b.push(0);
    b.extend(b"response-head\0HTTP/1.1 200 OK\0");
    b.extend((data as u32).to_be_bytes());
    let p = entries.join(name);
    fs::write(&p, b).unwrap();
    p
}

/// A Chromium-style user data folder under `local` (and the split Opera layout's
/// roaming half under `roaming`) with every store, for the site tests:
/// `youtube.com` in all of them, `example.com` in the Code Cache only.
pub fn chromium_profile(profile: &Path) {
    blockfile(
        &profile.join("Cache").join("Cache_Data"),
        &[
            (
                "1/0/_dk_https://youtube.com https://youtube.com https://i.ytimg.com/vi/a.jpg",
                5000,
            ),
            (
                "1/0/_dk_https://example.com https://example.com https://example.com/a.css",
                700,
            ),
        ],
    );
    let js = profile.join("Code Cache").join("js");
    simple_entry(
        &js,
        "_keyhttps://www.youtube.com/s/player.js \nhttps://youtube.com/",
        3000,
        false,
    );
    simple_entry(
        &js,
        "_keyhttps://m.youtube.com/m.js \nhttps://m.youtube.com/",
        100,
        true,
    );
    simple_entry(
        &js,
        "_keyhttps://example.com/app.js \nhttps://example.com/",
        2000,
        false,
    );
    simple_entry(&js, "_keychrome-extension://abcdef/bg.js \n", 900, false);
    simple_index(&js);
    let wasm = profile.join("Code Cache").join("wasm");
    simple_entry(
        &wasm,
        "_keyhttps://www.youtube.com/codec.wasm \nhttps://youtube.com/",
        4000,
        false,
    );
    simple_index(&wasm);
    let cs = profile.join("Service Worker").join("CacheStorage");
    cache_storage_origin(
        &cs,
        "379f1cbab5b08b6fc9e08681e42d8be311441c88",
        "https://www.youtube.com/",
        6000,
    );
    // what the cleaner must never touch
    for name in ["Cookies", "Login Data", "History", "Preferences"] {
        fs::write(profile.join(name), b"keep").unwrap();
    }
    for dir in [
        "Local Storage/leveldb",
        "IndexedDB/https_www.youtube.com_0.indexeddb.leveldb",
        "Extensions/abc",
        "Service Worker/Database",
    ] {
        fs::create_dir_all(profile.join(dir)).unwrap();
        fs::write(profile.join(dir).join("000003.log"), b"keep").unwrap();
    }
}

/// A Firefox profile's local half (`cache2`) with `youtube.com` and `mozilla.org`.
pub fn firefox_profile(profile: &Path) {
    let entries = profile.join("cache2").join("entries");
    firefox_entry(&entries, "a,:https://www.youtube.com/", 1500, 3);
    firefox_entry(
        &entries,
        "O^partitionKey=%28https%2Cyoutube.com%29,a,:https://i.ytimg.com/x.jpg",
        2500,
        3,
    );
    firefox_entry(&entries, "a,:https://www.mozilla.org/", 800, 2);
    fs::write(profile.join("cache2").join("index"), [7u8; 64]).unwrap();
}
