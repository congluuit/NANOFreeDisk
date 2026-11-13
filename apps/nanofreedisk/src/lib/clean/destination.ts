// Where removed items go (docs/PHASE3_CLEAN.md, "Simple mode: one destination").
// Simple mode has one rule for every tool: the app's quarantine, restorable from
// Cleanup history for 7 days. Expert mode keeps each tool's own default. The
// executor still decides per item: a drive without a quarantine (FAT32 stick,
// network share) falls back to the Recycle Bin (`QuarantineRoots::root_for`).
// Pure, so destination.test.ts can check it.

import type { Volume } from "../types.ts";
import type { DeleteOutcome, OperationResult, Problem } from "./types.ts";

/** Where a removal sends its items. */
export type Destination = "quarantine" | "recycle-bin";
/** What the user sees the run as ("permanent" only from Expert options). */
export type ShownMode = Destination | "permanent";
/** Every tool that removes things the user picked. */
export type RemovalTool = "clean" | "tidy" | "manual" | "dupes";

/** Expert mode: each tool's own default, as before the one rule. */
export const EXPERT_DEFAULT: Record<RemovalTool, Destination> = {
  clean: "quarantine",
  tidy: "quarantine",
  manual: "recycle-bin",
  dupes: "recycle-bin",
};

/** Where `tool` sends removed items in the current mode. */
export function destinationFor(tool: RemovalTool, expert: boolean): Destination {
  return expert ? EXPERT_DEFAULT[tool] : "quarantine";
}

/** A removal this large asks where it should go (Simple mode). */
export const BIG_REMOVAL_BYTES = 20 * 2 ** 30;
/** ... and so does any removal from a drive with less than this share free. */
export const LOW_FREE_SHARE = 0.1;

/** Drive letter (`c:`) or share (`//server/share`) of a path, lower case. */
export function volumeKey(path: string): string | null {
  const p = path.replace(/\\/g, "/").replace(/^\/\/\?\//, "").toLowerCase();
  const share = /^\/\/([^/]+)\/([^/]+)/.exec(p);
  if (share) return `//${share[1]}/${share[2]}`;
  const drive = /^([a-z]:)/.exec(p);
  if (drive) return drive[1];
  return p.startsWith("/") ? "/" : null;
}

/** The volume holding `path` (longest matching root; `/` on macOS). */
export function volumeOf(path: string, volumes: Volume[]): Volume | null {
  const key = volumeKey(path);
  if (!key) return null;
  if (key !== "/") return volumes.find((v) => volumeKey(v.root) === key) ?? null;
  const p = path.toLowerCase();
  return (
    volumes
      .filter((v) => p.startsWith(v.root.toLowerCase()))
      .sort((a, b) => b.root.length - a.root.length)[0] ?? null
  );
}

/** Drives (of `paths`) with less than 10 % free space. */
export function lowDrives(paths: string[], volumes: Volume[]): Volume[] {
  const seen = new Map<string, Volume>();
  for (const p of paths) {
    const v = volumeOf(p, volumes);
    if (v && v.total > 0 && v.available / v.total < LOW_FREE_SHARE) seen.set(v.root, v);
  }
  return [...seen.values()];
}

/** Over 20 GB, or from a drive that is almost full: the quarantine keeps the space
 *  in use for 7 days, so the confirmation offers the Recycle Bin as a choice. */
export function isBigRemoval(bytes: number, paths: string[], volumes: Volume[]): boolean {
  return bytes >= BIG_REMOVAL_BYTES || lowDrives(paths, volumes).length > 0;
}

/** Locale key of the one-sentence safety note for `mode`. */
export function safetyNoteKey(mode: ShownMode): string {
  return mode === "recycle-bin" ? "safety-note-recycle" : mode === "permanent" ? "safety-note-permanent" : "safety-note";
}

/** Locale keys of the remove buttons for `dest`: "Remove" with the quarantine (the
 *  safety note says where), "Move to the Recycle Bin" otherwise. */
export function removeKeys(dest: Destination): { bar: string; one: string; many: string; hint: string } {
  return dest === "quarantine"
    ? { bar: "sel-remove-n", one: "action-remove", many: "action-remove-n", hint: "action-remove-hint" }
    : { bar: "sel-recycle-n", one: "action-recycle", many: "action-recycle-n", hint: "action-recycle-hint" };
}

/** Every key the shared confirmation and result use (checked in both languages). */
export const REMOVAL_KEYS: string[] = [
  "safety-note",
  "safety-note-recycle",
  "safety-note-permanent",
  "confirm-what",
  "confirm-where",
  "confirm-space-later",
  "confirm-big",
  "confirm-choice-quarantine",
  "confirm-choice-recycle",
  "run-title",
  "run-counts",
  "run-held",
  "run-fallback",
  "run-history",
  "run-stayed-title",
  "action-remove",
  "action-remove-n",
  "action-remove-hint",
  "sel-remove-n",
];

/** Turn the outcomes of a hand-picked delete into the shared result shape
 *  (`paths`: id → path from the plan). */
export function outcomesResult(id: string, outcomes: DeleteOutcome[], paths: Map<number, string>): OperationResult {
  const done = outcomes.filter((o) => o.status === "done");
  const problems: Problem[] = outcomes
    .filter((o) => o.status !== "done")
    .map((o) => ({
      path: paths.get(o.id) ?? String(o.id),
      status: o.status === "partial" ? "partial" : o.status === "failed" ? "failed" : o.status === "declined" ? "declined" : "skipped",
      reason: o.reason,
    }));
  return {
    id,
    freed: done.reduce((s, o) => s + o.onDisk, 0),
    done: done.length,
    partial: outcomes.filter((o) => o.status === "partial").length,
    skipped: outcomes.filter((o) => o.status === "blocked" || o.status === "declined").length,
    failed: outcomes.filter((o) => o.status === "failed").length,
    cancelled: false,
    problems,
    journalError: null,
  };
}
