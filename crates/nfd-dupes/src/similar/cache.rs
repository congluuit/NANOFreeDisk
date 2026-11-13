//! The picture-hash cache seen by the similar-photos search: the history database
//! ([`nfd_history::History`], table `image_hashes`) or an in-memory map in tests.

use std::collections::HashMap;

use nfd_history::{History, ImageHashRow};

use crate::cache::CacheKey;

/// Somewhere to remember picture hashes between runs.
pub trait ImageCache {
    /// Known rows of algorithm `algo` for `keys` (index-aligned).
    fn get(&self, keys: &[CacheKey], algo: u32) -> Vec<Option<ImageHashRow>>;
    /// Remember fresh rows. Failures are ignored: the cache only saves time.
    fn put(&mut self, rows: &[(CacheKey, ImageHashRow)]);
}

impl ImageCache for History {
    fn get(&self, keys: &[CacheKey], algo: u32) -> Vec<Option<ImageHashRow>> {
        self.image_hash_lookup(keys, algo)
            .unwrap_or_else(|_| vec![None; keys.len()])
    }

    fn put(&mut self, rows: &[(CacheKey, ImageHashRow)]) {
        if self
            .image_hash_store(rows, nfd_core::exec::unix_now())
            .is_ok()
        {
            let _ = self.image_hash_prune(nfd_history::image_hashes::MAX_ROWS);
        }
    }
}

/// A cache that lives as long as the value (tests).
#[derive(Debug, Default)]
pub struct MemoryImageCache {
    rows: HashMap<(u64, u128), (CacheKey, ImageHashRow)>,
}

impl MemoryImageCache {
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl ImageCache for MemoryImageCache {
    fn get(&self, keys: &[CacheKey], algo: u32) -> Vec<Option<ImageHashRow>> {
        keys.iter()
            .map(|k| {
                self.rows
                    .get(&(k.volume, k.file_id))
                    .filter(|(stored, row)| stored == k && row.algo == algo)
                    .map(|(_, row)| row.clone())
            })
            .collect()
    }

    fn put(&mut self, rows: &[(CacheKey, ImageHashRow)]) {
        for (k, r) in rows {
            self.rows.insert((k.volume, k.file_id), (*k, r.clone()));
        }
    }
}
