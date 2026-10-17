# Clean one website's browser cache

Junk files → **Web browser caches** → *Advanced: clean one website* (and `nfd browsers`)
lists the websites in each browser profile's caches and removes only the chosen
sites' entries. Code: `crates/nfd-browsers`, `apps/nanofreedisk/src-tauri/src/commands/browsers.rs`,
`apps/nanofreedisk/src/lib/components/BrowserSites.svelte`, `apps/nfd-cli/src/browsers.rs`.

## What is read

| Store (per profile) | Format | Per-site cleaning |
|---|---|---|
| `Cache/Cache_Data` (HTTP cache, Chromium on Windows) | blockfile: `index`, `data_0` (rankings), `data_1` (256-byte entries), `data_2/3`, `f_xxxxxx` | **listed only** |
| `Cache/Cache_Data` where a browser uses the Simple backend | Simple | whole entries |
| `Code Cache/js`, `Code Cache/wasm` | Simple: `<16 hex>_0` (+ `_1`, `_s`), `index-dir/the-real-index` | whole entries |
| `Service Worker/CacheStorage/<origin hash>` | `index.txt` (`CacheStorageIndex` protobuf) + one Simple cache per cache | whole origin folder |
| Firefox `cache2/entries` | one file per entry, metadata (and the key) at the end | whole entries |

Opera keeps `Cache` under `%LOCALAPPDATA%` and the rest under `%APPDATA%`; the other
Chromium browsers keep everything in `User Data\<profile>`. Firefox's cache lives in
`%LOCALAPPDATA%\Mozilla\Firefox\Profiles\<profile>\cache2`. GPU and shader caches hold
no URLs and are not attributed to sites.

### Formats (as implemented, checked against real profiles read-only)

- **Simple entry file**: `SimpleFileHeader` = magic `0xfcfb6d1ba7725c30` (LE u64),
  version (u32, 5 today), key length (u32), key hash (u32), padding → the key starts
  at byte 24.
- **Code Cache key**: `_key<resource URL> \n<origin lock>`; the origin lock is the site.
- **HTTP cache key**: `1/0/_dk_<top-frame site> <frame site> <URL>` with the split
  cache, or just `<URL>`; the top-frame site is the site.
- **Blockfile**: `IndexHeader` is 368 bytes (magic `0xC103CAC3`, version 2.0 / 2.1 / 3.0,
  `table_len` at offset 28), then the hash table of `CacheAddr`s. A `CacheAddr` has the
  initialized bit (31), the file type (28–30; 0 = external `f_%06x`), the block count − 1
  (24–25), the file selector `data_N` (16–23) and the start block (0–15). Block files
  have an 8 KiB header (magic `0xC104CAC3`, entry size at 12, max entries at 20).
  `EntryStore`: next in chain at 4, state at 20 (0 = normal), key length at 32, long-key
  address at 36, four data sizes at 40, the key inline at 96 (spanning up to 4 blocks).
- **CacheStorage `index.txt`**: field 2 = origin, field 3 = storage key
  (`https://a.example/^0https://top.example` when partitioned: the top-level site wins).
- **Firefox cache2**: last 4 bytes (BE) = offset of the metadata = data size; metadata
  = hash (4), one 2-byte hash per 256 KiB chunk, header (version, fetch count, last
  fetched, last modified, frecency, expiration, key size, and `flags` from version 2;
  all BE u32), then the key and a NUL. Key: `[O^partitionKey=%28https%2Csite%29,]a,:<URL>`.

## Why the blockfile cache is never edited

Removing one blockfile entry means unlinking it from its hash chain (rewriting the
previous entry and its self-hash), from its rankings list (neighbours, list heads and
tails, sizes in the index header), freeing its blocks in each block file's allocation
bitmap and counters, and adjusting the index totals — consistently, without the
browser's own crash-recovery state ever seeing a half-done change. A mistake makes the
browser discard the whole cache. That is not "fully understood and safe", so per-site
cleaning shows what each site holds there (read-only) and leaves it; the UI and the CLI
say so, and the whole HTTP cache is still cleaned by the browser's rule in the Clean
view.

The Simple and Firefox caches are built for entries disappearing: their index is a
hint, a missing entry is a cache miss, and a missing (or older than its folder) index is
rebuilt from the entry files on the next start. NANOFreeDisk removes whole entry files
and the index together, so no index ever points at an entry that is gone.

## Safety

1. Only the stores above are read; planned paths must pass an allowlist relative to
   the profile (`Cache/Cache_Data/…`, `Code Cache/js|wasm/…`,
   `Service Worker/CacheStorage/<origin>`, `cache2/entries/…`, `cache2/index`) — never
   `Cookies`, `Login Data`, `History`, `Local Storage`, `IndexedDB`, `Extensions`,
   `Service Worker/Database` or anything else.
2. Every planned path must also match the browser's 🟢 Safe rule in
   `rules/browsers.toml` (`Matcher::rules_matching`): per-site cleaning is a strict
   subset of what the Clean view already cleans as a whole.
3. An entry goes as a whole (all of `<hash>_0/_1/_s`) or not at all.
4. Nothing is planned while the browser's exe runs; while items move the running
   processes are checked again every 200 items (refusal code `app-running`).
5. Everything goes Planner → SafetyGate → Executor → UndoJournal into the quarantine
   (kind `browser-site`); Restore in Activity puts the files back byte for byte.
6. Tests use synthetic caches (`nfd_browsers::fixture`, `scripts/gui-smoke.mjs`); real
   profiles were only listed read-only.
