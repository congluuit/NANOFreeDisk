// Pure helpers of "Similar photos": what the user ticked, what "select all but the
// best" picks, the totals and the groups that would lose every photo. Nothing is
// ever ticked without the user asking: similar is not identical.

import type { Photo, SimilarGroup, SimilarSelection } from "./types";

/** What the user picked in one group. */
export interface Pick {
  remove: string[];
  /** Ticked "remove all photos of this group" after removing every one. */
  keepNone: boolean;
}

export type Picks = Record<number, Pick>;

/** Risky to remove (cloud-sync, git, application folder, read-only, shared clone):
 * never ticked by a helper, extra confirmation. */
export function risky(p: Photo): boolean {
  return p.flags.cloud || p.flags.git || p.flags.appDir || p.flags.readonly || !!p.flags.clone;
}

/** "Select all but the best": every photo except the first, never a risky one. */
export function allButBest(g: SimilarGroup): string[] {
  return g.photos.slice(1).filter((p) => !risky(p)).map((p) => p.path);
}

/** Tick or untick `path` in `group`. Unticking a photo clears "keep none". */
export function togglePick(picks: Picks, group: number, path: string): Picks {
  const p = picks[group] ?? { remove: [], keepNone: false };
  const on = p.remove.includes(path);
  const remove = on ? p.remove.filter((x) => x !== path) : [...p.remove, path];
  return { ...picks, [group]: { remove, keepNone: on ? false : p.keepNone } };
}

/** Replace the picks of `group` (an empty list removes the group's entry). */
export function setGroupPicks(picks: Picks, group: number, remove: string[]): Picks {
  const out = { ...picks };
  if (remove.length) out[group] = { remove: [...remove], keepNone: false };
  else delete out[group];
  return out;
}

/** `true` when `remove` takes every photo of `g`. */
export function wouldEmpty(g: SimilarGroup, remove: string[]): boolean {
  return g.photos.length > 0 && g.photos.every((p) => remove.includes(p.path));
}

/** Selections for the backend. A group that would lose every photo goes only with
 * its "keep none" tick; otherwise it is left out and listed in `blocked`. */
export function selections(groups: SimilarGroup[], picks: Picks): { ok: SimilarSelection[]; blocked: number[] } {
  const ok: SimilarSelection[] = [];
  const blocked: number[] = [];
  for (const g of groups) {
    const p = picks[g.id];
    if (!p?.remove.length) continue;
    const remove = g.photos.filter((x) => p.remove.includes(x.path)).map((x) => x.path);
    if (!remove.length) continue;
    if (wouldEmpty(g, remove)) {
      if (p.keepNone) ok.push({ group: g.id, remove, keepNone: true });
      else blocked.push(g.id);
    } else {
      ok.push({ group: g.id, remove });
    }
  }
  return { ok, blocked };
}

/** Photos ticked (in groups that may go), their size, the risky ones and the
 * groups emptied with "keep none". */
export function totals(groups: SimilarGroup[], picks: Picks): { files: number; bytes: number; groups: number; risky: Photo[]; emptied: number; blocked: number } {
  const { ok, blocked } = selections(groups, picks);
  let files = 0;
  let bytes = 0;
  let emptied = 0;
  const flagged: Photo[] = [];
  for (const s of ok) {
    const g = groups.find((x) => x.id === s.group);
    if (!g) continue;
    if (s.keepNone) emptied++;
    for (const p of g.photos) {
      if (!s.remove.includes(p.path)) continue;
      files++;
      bytes += p.size;
      if (risky(p)) flagged.push(p);
    }
  }
  return { files, bytes, groups: ok.length, risky: flagged, emptied, blocked: blocked.length };
}

/** Drop picks of photos and groups that are no longer listed. */
export function prunePicks(groups: SimilarGroup[], picks: Picks): Picks {
  const out: Picks = {};
  for (const g of groups) {
    const p = picks[g.id];
    if (!p) continue;
    const remove = p.remove.filter((path) => g.photos.some((x) => x.path === path));
    if (remove.length) out[g.id] = { remove, keepNone: p.keepNone && wouldEmpty(g, remove) };
  }
  return out;
}

/** Why the first photo of a group is suggested: more pixels, a larger file, or
 * (same picture and size) where it lives. */
export function bestReason(g: SimilarGroup): "pixels" | "size" | "place" {
  const [a, b] = g.photos;
  if (!a || !b) return "pixels";
  if (a.width * a.height > b.width * b.height) return "pixels";
  if (a.size > b.size) return "size";
  return "place";
}

/** "4000 × 3000". */
export function resolution(p: Photo): string {
  return `${p.width} × ${p.height}`;
}

/** Megapixels with one decimal ("12.0"). */
export function megapixels(p: Photo, locale = "en"): string {
  return new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format((p.width * p.height) / 1e6);
}

/** The folder of a path. */
export function folderOf(path: string): string {
  const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return i > 0 ? path.slice(0, i) : path;
}

/** The file name of a path. */
export function nameOf(path: string): string {
  return path.slice(Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/")) + 1);
}
