//! The hash cache seen by the pipeline: the history database
//! ([`nfd_history::History`], table `file_hashes`) or an in-memory map in tests.

use std::collections::HashMap;

use nfd_history::{HashKey, HashRow, History};

/// Identity of a file's contents: same volume, file id, size and last write.
pub type CacheKey = HashKey;
/// Digests known for it.
pub type CachedHash = HashRow;

/// Somewhere to remember hashes between runs.
pub trait HashCache {
    /// Known hashes for `keys` (index-aligned).
    fn get(&self, keys: &[CacheKey]) -> Vec<Option<CachedHash>>;
    /// Remember freshly computed hashes. Failures are ignored: the cache only saves
    /// time.
    fn put(&mut self, rows: &[(CacheKey, CachedHash)]);
}

impl HashCache for History {
    fn get(&self, keys: &[CacheKey]) -> Vec<Option<CachedHash>> {
        self.hash_lookup(keys)
            .unwrap_or_else(|_| vec![None; keys.len()])
    }

    fn put(&mut self, rows: &[(CacheKey, CachedHash)]) {
        if self.hash_store(rows, nfd_core::exec::unix_now()).is_ok() {
            let _ = self.hash_prune(nfd_history::hashes::MAX_ROWS);
        }
    }
}

/// A cache that lives as long as the value (tests, history turned off).
#[derive(Debug, Default)]
pub struct MemoryCache {
    rows: HashMap<(u64, u128), (CacheKey, CachedHash)>,
}

impl MemoryCache {
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl HashCache for MemoryCache {
    fn get(&self, keys: &[CacheKey]) -> Vec<Option<CachedHash>> {
        keys.iter()
            .map(|k| {
                self.rows
                    .get(&(k.volume, k.file_id))
                    .filter(|(stored, _)| stored == k)
                    .map(|(_, h)| *h)
            })
            .collect()
    }

    fn put(&mut self, rows: &[(CacheKey, CachedHash)]) {
        for (k, h) in rows {
            self.rows.insert((k.volume, k.file_id), (*k, *h));
        }
    }
}
