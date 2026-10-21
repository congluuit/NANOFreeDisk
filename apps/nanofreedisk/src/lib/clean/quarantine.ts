// What the Activity log shows of the quarantine (tested in quarantine.test.ts):
// the same tests as nfd_history's `ItemRow::restorable` / `in_quarantine`, and the
// days left before an operation's files are deleted by themselves.

import type { OperationItem } from "./types.ts";

/** Journal modes whose items the quarantine holds on disk (files or `.reg` backups). */
export const HELD_MODES: readonly OperationItem["mode"][] = ["quarantine", "hardlink", "clone", "registry"];

/** Can still be put back. */
export function restorableItem(i: OperationItem, state: string): boolean {
  return !i.restored && !i.purged && state !== "purged" && (i.status === "done" || i.status === "partial") && i.mode !== "permanent";
}

/** Held in the quarantine (not the Recycle Bin): it can also be deleted for good. */
export function heldItem(i: OperationItem, state: string): boolean {
  return restorableItem(i, state) && i.stored !== null && HELD_MODES.includes(i.mode);
}

/** Whole days until `at` (Unix seconds), 0 once it is today or past. */
export function daysLeft(at: number, now = Date.now() / 1000): number {
  return Math.max(0, Math.ceil((at - now) / 86_400));
}
