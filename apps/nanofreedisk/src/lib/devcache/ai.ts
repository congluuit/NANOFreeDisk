// AI leftovers tab: pure helpers over the scan report (tested in ai.test.ts).

import type { AiReport } from "./types";

/** Blocked codes the AI leftovers scan uses besides the SafetyGate's (each has a
 *  `reason-*` message). */
export const AI_BLOCKED = [
  "tracked",
  "contains-history",
  "too-large",
  "no-git",
  "git-failed",
  "dirty",
  "not-merged",
  "locked",
  "too-many-ignored",
  "foreign-worktree",
  "ignored-left",
  "not-a-worktree",
] as const;

/** Why a leftover is listed but not pre-selected (each has an `ail-keep-*` message). */
export const AI_KEEP = ["session-open", "session-recent", "recent-write"] as const;

/** Paths "Clean finished sessions (safe only)" picks: pre-selected items and
 *  pre-selected build output of worktrees that cannot be removed. */
export function aiPreselected(r: AiReport | null): string[] {
  return (r?.items ?? []).flatMap((i) => [
    ...(i.selected ? [i.path] : []),
    ...(i.worktree?.artifacts ?? []).filter((a) => a.selected && !a.blocked).map((a) => a.path),
  ]);
}

/** Bytes of the ticked paths (items and worktree build output). */
export function aiBytes(r: AiReport | null, paths: string[]): number {
  const set = new Set(paths);
  let sum = 0;
  for (const i of r?.items ?? []) {
    if (set.has(i.path)) sum += i.bytes;
    for (const a of i.worktree?.artifacts ?? []) if (set.has(a.path)) sum += a.bytes;
  }
  return sum;
}
