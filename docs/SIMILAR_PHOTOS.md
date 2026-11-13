# Similar photos

"Similar photos" is the second half of the Duplicates view (the switch at its top:
*Identical files* / *Similar photos*). It finds pictures that look the same without
being the same file: the same photo at another resolution, compressed again by a
chat app (Zalo, Messenger, WhatsApp), exported twice, saved with other metadata or
slightly cropped. Similar is not identical, so nothing is ever ticked for the user.

Code: `crates/nfd-dupes/src/similar.rs` (search and groups), `similar/fingerprint.rs`
(the hash), `similar/decode.rs` (decoders and limits), `similar/cache.rs`,
`similar/plan.rs` (removal), `crates/nfd-history/src/image_hashes.rs` (cache table),
`apps/nanofreedisk/src-tauri/src/commands/similar.rs`, `src/lib/similar/` and
`src/lib/components/dupes/SimilarPhotos.svelte`. CLI: `nfd dupes --similar
[--similarity strict|normal|loose] [--json]` lists the groups and never removes
anything.

## Prior art

czkawka's similar images (MIT, core `similar_images`): image_hasher gradient hash at
8 to 64 bits per side, a BK-tree over Hamming distances, and presets per hash size
(for 16 × 16: 2, 5, 15, 30 bits). dupeGuru's picture mode compares downscaled
blocks with a percentage threshold. The approach here follows czkawka's default
(gradient hash, 16 × 16), adds a vertical half and a coarse prefilter, and is our own
code; no code was copied.

## Pipeline

1. **Candidates**: `collect::collect`, the same pass as identical files, so every
   exclusion holds: SafetyGate-protected places (AI conversation history included),
   cloud placeholders (never opened), links, SYSTEM files, application folders
   (Expert can list them, never suggest them). Only `jpg jpeg jpe jfif png webp gif
   bmp`, from 10 KiB (icons, stickers) up to 64 MiB. Hard links count once.
2. **Decode** (`decode.rs`), at most 4 pictures at a time per SSD, one at a time on
   spinning disks, threads in background mode. JPEG goes through `zune-jpeg`
   straight to brightness (no chroma, no colour conversion, one byte per pixel);
   none of the pure-Rust JPEG decoders can scale while decoding, so the picture is
   decoded at full size (about 40 ms for 12 MP) and averaged down. PNG, WebP, GIF
   (first frame) and BMP go through `image` with its `Limits`. The EXIF orientation
   is applied. Limits: 30,000 pixels per side, 100 MP for JPEG, 40 MP and 256 MiB of
   allocations for the other formats; a decoder error or panic means "unreadable".
   The file is checked (size, last write, not a placeholder) before it is opened and
   again after it was read.
3. **Fingerprint** (`fingerprint.rs`): brightness averaged into 17 × 16, 16 × 17 and
   9 × 8 grids (every pixel counts, integer bins, one pass). A bit is set when the
   next cell is brighter by more than 2 levels: 256 horizontal + 256 vertical bits
   (fine) and 64 horizontal bits (coarse). Pictures whose grid varies less than 3
   levels (a plain colour, a blank page) or under 32 pixels are left out as "no
   detail".
4. **Groups**: two pictures look alike when their aspect ratios differ by at most
   1.2 times, the coarse hashes by at most C bits and the fine hashes by at most F
   bits (table below). Pictures are taken best first (most pixels, then largest
   file); each one not yet grouped starts a group with every later look-alike of
   itself. Every member is close to the first photo, so A and C never join through
   B. Changing the preset regroups the cached fingerprints without reading a file.
5. **Identical copies** inside a group (same size and fingerprint, then the same
   BLAKE3) are marked *Identical copy* and left out of the group's size: they belong
   to *Identical files*.

### Why this hash

The gradient hash compares neighbours, not absolute values, so it is blind to
brightness, contrast, resolution and JPEG noise; a mirrored or rotated picture
changes about half of its bits. A 64-bit pHash was tried too: a small watermark
moved it by 22 to 26 bits of 64 and unrelated text pages came within 12 bits, so it
was dropped. The 2-level margin keeps flat areas (sky, walls, a smooth background)
at 0 instead of following compression noise: it cut the distance of a re-compressed
synthetic picture from 84 to 18 bits while unrelated pictures stayed above 130.

### Thresholds

| Preset | Fine (of 512) | Coarse (of 64) | Catches |
|---|---|---|---|
| **Strict** (default) | 40 | 10 | resized, re-compressed (chat apps, JPEG quality 15 at half size), re-exported, metadata changes, small watermarks |
| Normal | 64 | 14 | also stronger brightness edits and many crops of a few percent |
| Loose | 96 | 20 | also most crops of a few percent and visible edits; bursts of one scene may join |

Calibration (Windows wallpapers at several sizes plus synthetic pages and
screenshots): the variants above stayed at or under 39 / 67 / 97 bits, while the
closest unrelated pair was 122 bits apart (two screenshots of the same layout) and
unrelated photos 132 or more. The default is **Strict** because the realistic false
positive is not two random photos but two screenshots or scans of the same layout
with different text: they can come within a few dozen bits, and the text is what
matters. Strict still catches every chat-app copy we measured.

### Scale

Every pair is compared once, coarse hash first (one `popcount`), on 4 background
threads: about 5 × 10⁹ cheap tests for 100,000 pictures, a few seconds. A BK-tree
does not help at these radii: the radius is close to the spread of distances between
unrelated pictures, so a query visits nearly every node anyway.

## Cache

Table `image_hashes` in the history database next to `file_hashes`, keyed by volume
and file id, used only while size and last write are unchanged, the row was written
at least two seconds after the last write, and the algorithm version matches. It
also remembers pictures that could not be compared (unreadable, too large, no
detail) so a broken file is not decoded every time. Capped at 1,000,000 rows, wiped
with the scan history. `useCache: false` (the GUI smoke test) skips it.

## What is never pre-selected

Nothing. The view opens with every box empty. *Keep only the best* ticks one group
on request and never ticks the best photo or a photo in a cloud-sync, git,
application or read-only folder. A group can only lose every photo after its own
*Remove all photos of this group* tick.

## Removal

`similar::plan::plan` builds an ordinary hand-picked deletion (`Origin::Manual`):
each photo must be in its group and pass the SafetyGate's manual rules (what
`safety::check_manual_delete` checks), a selection that empties a group without
`keepNone` is refused, and replacing a photo by a hard link or clone is refused. Each item carries the size and last write
of the search; right before the move the gate refuses it if either changed, and
`SimilarVerifier` refuses a photo that became a cloud placeholder and, unless the
group was confirmed with "keep none", any photo whose group no longer has a photo
that stays on disk unchanged. The exact search's byte-for-byte verifier does not
apply (the files differ by design). Destination as for identical files: the
quarantine in Simple mode, the Recycle Bin (or the quarantine) in Expert mode;
Cleanup history lists the run as *Similar photos removed*.

## Limits

- HEIC, RAW and TIFF photos are not compared (no small pure-Rust decoder).
- Only the first frame of an animated GIF is compared.
- Mirrored or rotated copies (without an EXIF tag) are different pictures here.
- Grid previews are decoded on demand (320 pixels, two at a time); the side-by-side
  view shows files up to 8 MiB as they are and a 1,600-pixel preview above that.
