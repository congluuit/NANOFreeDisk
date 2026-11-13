// Browser-only stand-in for the Recycle Bin commands (`pnpm dev` without Tauri):
// two drives with something in their bin; emptying clears them. Nothing touches
// a real disk.

import type { BinDrive } from "./types";

const MB = 2 ** 20;
let drives: BinDrive[] = [
  { root: "C:\\", bytes: 1240 * MB, items: 342 },
  { root: "D:\\", bytes: 86 * MB, items: 12 },
];

const total = () => ({
  bytes: drives.reduce((s, d) => s + d.bytes, 0),
  items: drives.reduce((s, d) => s + d.items, 0),
});

export const recycleBinHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  recyclebin_status: () => ({ ...total(), drives: drives.filter((d) => d.items > 0), settled: 0 }),
  recyclebin_empty: async ({ root }) => {
    await new Promise((r) => setTimeout(r, 400));
    const hit = drives.filter((d) => !root || d.root.toLowerCase() === String(root).toLowerCase());
    const out = { bytes: hit.reduce((s, d) => s + d.bytes, 0), items: hit.reduce((s, d) => s + d.items, 0), failed: [], settled: 0, journalError: null };
    drives = drives.filter((d) => !hit.includes(d));
    return out;
  },
};
