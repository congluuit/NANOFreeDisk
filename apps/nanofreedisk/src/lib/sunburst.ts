// Sunburst (radial partition) layout over a `Slice`: depth → ring, size → angle.

import type { Slice } from "./types";

export interface Arcs {
  start: Float64Array;
  end: Float64Array;
  depth: Uint8Array;
  /** Slice indices with an arc, per depth, sorted by start angle (for hit tests). */
  rings: number[][];
  maxDepth: number;
}

const TAU = Math.PI * 2;

export function layoutSunburst(slice: Slice, maxDepth: number, minAngle = 0.004): Arcs {
  const n = slice.ids.length;
  const start = new Float64Array(n).fill(NaN);
  const end = new Float64Array(n).fill(NaN);
  const depth = new Uint8Array(n);
  const rings: number[][] = Array.from({ length: maxDepth + 1 }, () => []);
  if (!n) return { start, end, depth, rings, maxDepth };
  start[0] = 0;
  end[0] = TAU;
  // cursor per parent: children follow in slice order (already size-descending)
  const cursor = new Float64Array(n);
  cursor[0] = 0;
  let deepest = 0;
  for (let i = 1; i < n; i++) {
    const p = slice.parents[i];
    if (p < 0 || Number.isNaN(start[p])) continue;
    const d = slice.depths[i];
    if (d > maxDepth) continue;
    const parentSize = slice.sizes[p];
    if (parentSize <= 0) continue;
    const span = ((end[p] - start[p]) * slice.sizes[i]) / parentSize;
    const s = start[p] + cursor[p];
    cursor[p] += span;
    if (span < minAngle) continue;
    start[i] = s;
    end[i] = s + span;
    depth[i] = d;
    rings[d].push(i);
    deepest = Math.max(deepest, d);
  }
  for (const r of rings) r.sort((a, b) => start[a] - start[b]);
  return { start, end, depth, rings, maxDepth: deepest };
}

/** Slice index of the arc under polar coordinates, or -1 (0 = center disc). */
export function hitArc(arcs: Arcs, angle: number, ring: number): number {
  if (ring === 0) return 0;
  const list = arcs.rings[ring];
  if (!list) return -1;
  const a = ((angle % TAU) + TAU) % TAU;
  let lo = 0;
  let hi = list.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const i = list[mid];
    if (a < arcs.start[i]) hi = mid - 1;
    else if (a >= arcs.end[i]) lo = mid + 1;
    else return i;
  }
  return -1;
}
