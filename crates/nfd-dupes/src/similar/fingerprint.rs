//! The fingerprint of a picture: gradient hashes ("dHash") of its brightness.
//!
//! The picture (already turned upright and reduced to brightness) is averaged into
//! small grids, every cell the plain mean of the pixels it covers (integer bins, so
//! the whole picture counts and nothing is sampled). Each bit then says whether the
//! next cell is clearly brighter (by more than two levels, so flat areas do not
//! follow compression noise):
//!
//! * **fine**, 512 bits: 16 × 16 horizontal comparisons on a 17 × 16 grid plus
//!   16 × 16 vertical comparisons on a 16 × 17 grid. Two pictures look alike when
//!   few of these bits differ ([`fine_distance`]).
//! * **coarse**, 64 bits: 8 × 8 horizontal comparisons on a 9 × 8 grid. A cheap
//!   first test that rejects almost every unrelated pair with one `popcount`.
//!
//! Comparing neighbours instead of absolute values makes the hash blind to a
//! brightness or contrast change, to the resolution and to JPEG re-compression,
//! while a mirrored or rotated picture changes about half of the bits. The same
//! algorithm at 16 × 16 is czkawka's default ("Gradient"); the vertical half catches
//! pictures that only differ upside down. Calibration: `docs/SIMILAR_PHOTOS.md`.

/// Number of `u64` words of the fine hash.
pub const FINE_WORDS: usize = 8;
/// Bytes of a fingerprint in the cache: the fine words, then the coarse word.
pub const BYTES: usize = (FINE_WORDS + 1) * 8;
/// Pictures whose grid brightness varies less than this (standard deviation, in
/// 0..255 units) have no detail to compare: a plain colour, a blank page.
pub const PLAIN_SD: f64 = 3.0;
/// Pictures narrower or lower than this are icons and buttons, not photos.
pub const MIN_SIDE: usize = 32;
/// A cell counts as brighter than its neighbour only by more than this many
/// brightness levels (× 64, the fixed point of the cell means). Without the margin
/// the bits of flat areas (sky, walls, a smooth background) follow JPEG noise; with
/// it they stay 0. Two levels halved the distance of re-compressed copies in the
/// calibration while unrelated pictures stayed as far apart.
const MARGIN: u64 = 2 * 64;

/// What two pictures are compared by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    pub fine: [u64; FINE_WORDS],
    pub coarse: u64,
}

impl Fingerprint {
    /// Little-endian bytes for the cache.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(BYTES);
        for w in self.fine.iter().chain(std::iter::once(&self.coarse)) {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out
    }

    /// The fingerprint [`Self::to_bytes`] wrote (`None` for anything else).
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != BYTES {
            return None;
        }
        let word = |i: usize| {
            let mut w = [0u8; 8];
            w.copy_from_slice(&b[i * 8..i * 8 + 8]);
            u64::from_le_bytes(w)
        };
        let mut fine = [0u64; FINE_WORDS];
        for (i, f) in fine.iter_mut().enumerate() {
            *f = word(i);
        }
        Some(Self {
            fine,
            coarse: word(FINE_WORDS),
        })
    }
}

/// Bits that differ between two fine hashes (0..=512).
#[must_use]
pub fn fine_distance(a: &Fingerprint, b: &Fingerprint) -> u32 {
    a.fine
        .iter()
        .zip(&b.fine)
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

/// Bits that differ between two coarse hashes (0..=64).
#[must_use]
pub const fn coarse_distance(a: &Fingerprint, b: &Fingerprint) -> u32 {
    (a.coarse ^ b.coarse).count_ones()
}

/// Cell index of every source coordinate for `parts` integer bins over `n` pixels
/// (`n >= parts`), and the width of every bin.
fn bins(n: usize, parts: usize) -> (Vec<usize>, Vec<u64>) {
    let mut index = vec![0usize; n];
    let mut width = vec![0u64; parts];
    for (j, wj) in width.iter_mut().enumerate() {
        let (a, b) = (j * n / parts, (j + 1) * n / parts);
        for slot in &mut index[a..b] {
            *slot = j;
        }
        *wj = (b - a) as u64;
    }
    (index, width)
}

/// Mean brightness (× 64) of every cell of one grid.
struct Grid {
    cols: usize,
    x: Vec<usize>,
    y: Vec<usize>,
    wx: Vec<u64>,
    wy: Vec<u64>,
    sum: Vec<u64>,
}

impl Grid {
    fn new(w: usize, h: usize, cols: usize, rows: usize) -> Self {
        let (x, wx) = bins(w, cols);
        let (y, wy) = bins(h, rows);
        Self {
            cols,
            x,
            y,
            wx,
            wy,
            sum: vec![0; cols * rows],
        }
    }

    fn mean(&self, row: usize, col: usize) -> u64 {
        let n = self.wx[col] * self.wy[row];
        self.sum[row * self.cols + col] * 64 / n.max(1)
    }
}

/// Fingerprint of a `w` × `h` brightness picture (row-major, one byte per pixel),
/// with the standard deviation of its 17 × 16 grid. `None` when the picture is
/// smaller than [`MIN_SIDE`] or `px` does not match the size.
#[must_use]
pub fn fingerprint(px: &[u8], w: usize, h: usize) -> Option<(Fingerprint, f64)> {
    if w < MIN_SIDE || h < MIN_SIDE || px.len() != w.checked_mul(h)? {
        return None;
    }
    let mut gh = Grid::new(w, h, 17, 16);
    let mut gv = Grid::new(w, h, 16, 17);
    let mut gc = Grid::new(w, h, 9, 8);
    // one pass over the pixels feeds the three grids
    for (y, row) in px.chunks_exact(w).enumerate() {
        let (rh, rv, rc) = (gh.y[y] * 17, gv.y[y] * 16, gc.y[y] * 9);
        for (x, &p) in row.iter().enumerate() {
            let p = u64::from(p);
            gh.sum[rh + gh.x[x]] += p;
            gv.sum[rv + gv.x[x]] += p;
            gc.sum[rc + gc.x[x]] += p;
        }
    }
    let mut fine = [0u64; FINE_WORDS];
    let mut set = |bit: usize| fine[bit / 64] |= 1 << (bit % 64);
    for r in 0..16 {
        for c in 0..16 {
            if gh.mean(r, c) + MARGIN < gh.mean(r, c + 1) {
                set(r * 16 + c);
            }
            if gv.mean(r, c) + MARGIN < gv.mean(r + 1, c) {
                set(256 + r * 16 + c);
            }
        }
    }
    let mut coarse = 0u64;
    for r in 0..8 {
        for c in 0..8 {
            if gc.mean(r, c) + MARGIN < gc.mean(r, c + 1) {
                coarse |= 1 << (r * 8 + c);
            }
        }
    }
    // spread of the cell means, in brightness units
    let cells: Vec<f64> = (0..16)
        .flat_map(|r| (0..17).map(move |c| (r, c)))
        .map(|(r, c)| gh.mean(r, c) as f64 / 64.0)
        .collect();
    let mean = cells.iter().sum::<f64>() / cells.len() as f64;
    let var = cells.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / cells.len() as f64;
    Some((Fingerprint { fine, coarse }, var.sqrt()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: usize, h: usize, f: impl Fn(usize, usize) -> u8) -> Vec<u8> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| f(x, y))
            .collect()
    }

    #[test]
    fn bins_cover_every_pixel_once() {
        for (n, parts) in [(17, 17), (100, 17), (4032, 16), (33, 9)] {
            let (index, width) = bins(n, parts);
            assert_eq!(width.iter().sum::<u64>(), n as u64);
            assert!(width.iter().all(|&w| w >= 1));
            assert!(index.windows(2).all(|p| p[0] <= p[1]));
            assert_eq!(index[n - 1], parts - 1);
        }
    }

    #[test]
    fn a_left_to_right_ramp_sets_every_horizontal_bit() {
        let px = picture(340, 160, |x, _| (x * 255 / 339) as u8);
        let (fp, sd) = fingerprint(&px, 340, 160).expect("fingerprint");
        assert_eq!(&fp.fine[..4], &[u64::MAX; 4]);
        // no vertical change at all
        assert_eq!(&fp.fine[4..], &[0; 4]);
        assert_eq!(fp.coarse, u64::MAX);
        assert!(sd > 50.0);
    }

    #[test]
    fn plain_and_tiny_pictures() {
        let px = vec![200u8; 64 * 64];
        let (_, sd) = fingerprint(&px, 64, 64).expect("fingerprint");
        assert!(sd < PLAIN_SD);
        assert!(fingerprint(&[0; 16 * 16], 16, 16).is_none());
        assert!(fingerprint(&[0; 10], 64, 64).is_none());
    }

    #[test]
    fn bytes_round_trip() {
        let fp = Fingerprint {
            fine: [1, 2, 3, 4, 5, 6, 7, u64::MAX],
            coarse: 0xabcd,
        };
        let b = fp.to_bytes();
        assert_eq!(b.len(), BYTES);
        assert_eq!(Fingerprint::from_bytes(&b), Some(fp));
        assert_eq!(Fingerprint::from_bytes(&b[1..]), None);
        assert_eq!(fine_distance(&fp, &fp), 0);
        let mut other = fp;
        other.fine[7] = 0;
        other.coarse = 0xabcc;
        assert_eq!(fine_distance(&fp, &other), 64);
        assert_eq!(coarse_distance(&fp, &other), 1);
    }
}
