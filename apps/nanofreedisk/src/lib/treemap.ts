// Nested squarified treemap layout (Bruls, Huizing & van Wijk 2000) over a `Slice`.
// Pure and allocation-light: 50k nodes lay out in a few milliseconds.

import type { Slice } from "./types";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Layout {
  /** Rectangle per slice index (NaN width when the node got no room). */
  x: Float32Array;
  y: Float32Array;
  w: Float32Array;
  h: Float32Array;
  /** Slice indices in paint order (parents before children). */
  order: Int32Array;
  count: number;
}

export interface LayoutOptions {
  /** Padding inside a directory frame. */
  pad: number;
  /** Height of a directory's label strip (only when the frame is large enough). */
  header: number;
  /** Do not descend into frames smaller than this (px). */
  minFrame: number;
}

const DEFAULTS: LayoutOptions = { pad: 2, header: 16, minFrame: 6 };

/** Lay the slice out inside `bounds`. Index 0 (the root) fills the bounds. */
export function layoutTreemap(slice: Slice, bounds: Rect, opts: Partial<LayoutOptions> = {}): Layout {
  const o = { ...DEFAULTS, ...opts };
  const n = slice.ids.length;
  const x = new Float32Array(n).fill(NaN);
  const y = new Float32Array(n).fill(NaN);
  const w = new Float32Array(n).fill(NaN);
  const h = new Float32Array(n).fill(NaN);
  const order = new Int32Array(n);
  let count = 0;
  if (n === 0) return { x, y, w, h, order, count };

  // children lists (slice is parent-before-child, children already size-descending)
  const first = new Int32Array(n).fill(-1);
  const next = new Int32Array(n).fill(-1);
  const last = new Int32Array(n).fill(-1);
  for (let i = 1; i < n; i++) {
    const p = slice.parents[i];
    if (p < 0) continue;
    if (first[p] < 0) first[p] = i;
    else next[last[p]] = i;
    last[p] = i;
  }

  x[0] = bounds.x;
  y[0] = bounds.y;
  w[0] = bounds.w;
  h[0] = bounds.h;
  order[count++] = 0;

  const kids: number[] = [];
  const stack = [0];
  while (stack.length) {
    const p = stack.pop()!;
    if (first[p] < 0) continue;
    const isRoot = p === 0;
    const pw = w[p];
    const ph = h[p];
    if (!(pw >= o.minFrame && ph >= o.minFrame)) continue;
    // inner area: root uses the whole bounds; nested dirs get padding + label strip
    const pad = isRoot ? 0 : o.pad;
    const head = !isRoot && pw > 60 && ph > o.header * 2.5 ? o.header : 0;
    let ix = x[p] + pad;
    let iy = y[p] + pad + head;
    let iw = pw - 2 * pad;
    let ih = ph - 2 * pad - head;
    if (iw < 2 || ih < 2) continue;

    kids.length = 0;
    let total = 0;
    for (let c = first[p]; c >= 0; c = next[c]) {
      if (slice.sizes[c] > 0) {
        kids.push(c);
        total += slice.sizes[c];
      }
    }
    if (total <= 0) continue;
    // scale sizes to area
    const scale = (iw * ih) / total;
    squarify(kids, slice.sizes, scale, ix, iy, iw, ih, x, y, w, h);
    for (const c of kids) {
      order[count++] = c;
      if (slice.kinds[c] === 1) stack.push(c);
    }
  }
  return { x, y, w, h, order, count };
}

function worst(sum: number, min: number, max: number, side: number): number {
  const s2 = sum * sum;
  const side2 = side * side;
  return Math.max((side2 * max) / s2, s2 / (side2 * min));
}

function squarify(
  items: number[],
  sizes: number[],
  scale: number,
  x0: number,
  y0: number,
  w0: number,
  h0: number,
  X: Float32Array,
  Y: Float32Array,
  W: Float32Array,
  H: Float32Array,
): void {
  let x = x0;
  let y = y0;
  let w = w0;
  let h = h0;
  let i = 0;
  while (i < items.length) {
    const side = Math.min(w, h);
    if (side <= 0) break;
    // grow the row while the worst aspect ratio improves
    let sum = sizes[items[i]] * scale;
    let min = sum;
    let max = sum;
    let j = i + 1;
    let cur = worst(sum, min, max, side);
    while (j < items.length) {
      const a = sizes[items[j]] * scale;
      const ns = sum + a;
      const nmin = Math.min(min, a);
      const nmax = Math.max(max, a);
      const nw = worst(ns, nmin, nmax, side);
      if (nw > cur) break;
      sum = ns;
      min = nmin;
      max = nmax;
      cur = nw;
      j++;
    }
    // place row i..j along the shorter side
    const thick = sum / side;
    let off = 0;
    if (w >= h) {
      // vertical strip on the left
      for (let k = i; k < j; k++) {
        const a = sizes[items[k]] * scale;
        const len = a / thick;
        const c = items[k];
        X[c] = x;
        Y[c] = y + off;
        W[c] = thick;
        H[c] = k === j - 1 ? h - off : len;
        off += len;
      }
      x += thick;
      w -= thick;
    } else {
      for (let k = i; k < j; k++) {
        const a = sizes[items[k]] * scale;
        const len = a / thick;
        const c = items[k];
        X[c] = x + off;
        Y[c] = y;
        W[c] = k === j - 1 ? w - off : len;
        H[c] = thick;
        off += len;
      }
      y += thick;
      h -= thick;
    }
    i = j;
  }
}

/** Topmost (deepest painted) slice index under the point, or -1. */
export function hitTest(layout: Layout, px: number, py: number): number {
  for (let k = layout.count - 1; k >= 0; k--) {
    const i = layout.order[k];
    const x = layout.x[i];
    const y = layout.y[i];
    if (px >= x && py >= y && px < x + layout.w[i] && py < y + layout.h[i]) return i;
  }
  return -1;
}

/** Bytes per pixel² threshold so that tiles below `minArea` px² are folded. */
export function minBytesFor(totalBytes: number, width: number, height: number, minArea = 6): number {
  if (width <= 0 || height <= 0) return 0;
  return (totalBytes * minArea) / (width * height);
}
