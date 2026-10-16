// Browser-only stand-in for the Duplicates commands (`pnpm dev` without Tauri): a
// photo library copied around Downloads, OneDrive and a git project. Removing
// "frees" the bytes; nothing touches a real disk.

import type { OperationResult } from "../clean/types";
import type { CopyFlags, DupeGroup, DupeReport, FileCopy, Selection } from "./types";

const MB = 2 ** 20;
const DAY = 86_400;
const now = () => Math.floor(Date.now() / 1000);
const HOME = "C:\\Users\\nano";
const PLAIN: CopyFlags = { cloud: false, git: false, appDir: false, readonly: false };

function copy(path: string, age: number, flags: Partial<CopyFlags> = {}): FileCopy {
  const volume = path.startsWith("D:") ? "00000000d0d0d0d0" : "00000000c0c0c0c0";
  return { path, mtime: now() - age * DAY, volume, fileId: null, flags: { ...PLAIN, ...flags }, link: "hardlink" };
}

function group(id: number, size: number, copies: FileCopy[]): DupeGroup {
  return { id, hash: id.toString(16).padStart(64, "0"), size, wasted: size * (copies.length - 1), copies };
}

let groups: DupeGroup[] = [];
function reset(): void {
  groups = [
    group(1, 2.4 * 2 ** 30, [
      copy(`D:\\Videos\\Wedding 2019.mp4`, 2000),
      copy(`${HOME}\\Downloads\\Wedding 2019.mp4`, 400),
      copy(`${HOME}\\OneDrive\\Videos\\Wedding 2019.mp4`, 390, { cloud: true }),
    ]),
    group(2, 4.7 * 2 ** 30, [copy(`D:\\ISO\\ubuntu-24.04.iso`, 300), copy(`${HOME}\\Downloads\\ubuntu-24.04.iso`, 120)]),
    group(3, 18 * MB, [copy(`D:\\Photos\\2021\\IMG_0042.jpg`, 1500), copy(`D:\\Photos\\Backup\\IMG_0042.jpg`, 900), copy(`${HOME}\\Desktop\\IMG_0042.jpg`, 30)]),
    group(4, 6 * MB, [copy(`D:\\dev\\site\\assets\\hero.png`, 200, { git: true }), copy(`${HOME}\\Downloads\\hero.png`, 190)]),
  ].map((g, i) => ({ ...g, id: i + 1 }));
  groups.sort((a, b) => b.wasted - a.wasted);
  groups.forEach((g, i) => (g.id = i + 1));
}
reset();

function report(roots: string[]): DupeReport & { totalGroups: number } {
  return {
    roots,
    minSize: MB,
    groups,
    wastedBytes: groups.reduce((s, g) => s + g.wasted, 0),
    scannedFiles: 48_210,
    candidates: 1_204,
    hashedBytes: 9.1 * 2 ** 30,
    cacheHits: 0,
    skipped: { small: 41_002, empty: 310, cloud: 12, protected: 4 },
    skippedSamples: [],
    elapsedMs: 8_400,
    paranoid: false,
    totalGroups: groups.length,
  };
}

const risky = (c: FileCopy) => c.flags.cloud || c.flags.git || c.flags.appDir || c.flags.readonly;
const disposable = (p: string) => /\\(downloads|desktop|temp)\\/i.test(p);

function select(strategy: string): Selection[] {
  return groups
    .map((g) => {
      const rank = (c: FileCopy): [number, number, number] => [
        risky(c) ? 0 : 1,
        strategy === "auto" && disposable(c.path) ? 1 : 0,
        strategy === "newest" ? -c.mtime : strategy === "shortest" ? c.path.length : c.mtime,
      ];
      const keep = [...g.copies].sort((a, b) => {
        const [x, y] = [rank(a), rank(b)];
        return x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
      })[0];
      return { group: g.id, keep: keep.path, remove: g.copies.filter((c) => c !== keep && !risky(c)).map((c) => c.path) };
    })
    .filter((s) => s.remove.length > 0);
}

let roots: string[] = [];

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const dupeHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  dupes_find: ({ paths }) => {
    reset();
    roots = paths?.length ? paths : ["C:\\"];
    return report(roots);
  },
  dupes_cancel: () => undefined,
  dupes_report: () => report(roots),
  dupes_select: ({ strategy }) => select(strategy),
  dupes_run: ({ selections }) => {
    let freed = 0;
    let done = 0;
    for (const s of selections as Selection[]) {
      const g = groups.find((x) => x.id === s.group);
      if (!g) continue;
      freed += g.size * s.remove.length;
      done += s.remove.length;
      g.copies = g.copies.filter((c) => !s.remove.includes(c.path));
      g.wasted = g.size * (g.copies.length - 1);
    }
    groups = groups.filter((g) => g.copies.length > 1);
    const r: OperationResult = { id: `mock-dupes-${now()}`, freed, done, partial: 0, skipped: 0, failed: 0, cancelled: false, problems: [], journalError: null };
    return r;
  },
  dupes_reveal: () => undefined,
  dupes_thumbnail: () => null,
  dupes_export: () => undefined,
  take_startup_dupes: () => null,
};
