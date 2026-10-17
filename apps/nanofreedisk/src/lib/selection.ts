// Shared selection model of every list the user ticks before acting (Clean, Dev
// caches, Duplicates, Old & large, Tidy up, Analyze, the per-site browser panel).
// Pure functions over a flat item list (tested in selection.test.ts); the views map
// their rows to `SelItem`s and keep their own picked-id arrays.
//
// Rules: 🔴 Protected and 🔒 Keep items are never selectable, 🟠 Advanced only in
// Expert mode, locked items (blocked by the gate, the copy a duplicate group keeps,
// an active AI session) never. Selecting here is only a tick: every action still
// goes Planner → SafetyGate → Executor.

export type SelLevel = "safe" | "caution" | "advanced" | "protected" | "keep";

/** Levels a "select only…" button exists for, in display order. */
export const PICK_LEVELS = ["safe", "caution", "advanced"] as const;
export type PickLevel = (typeof PICK_LEVELS)[number];

export interface SelItem {
  id: string;
  /** Bytes freed when it goes. */
  bytes: number;
  files?: number;
  /** `undefined` for lists without risk levels (hand-picked files). */
  level?: SelLevel | null;
  /** Group for the per-group checkbox (pack, vendor, project, duplicate group…). */
  group?: string;
  /** Never selectable (blocked, the copy that stays, protected). */
  locked?: boolean;
  /** Ticked but likely skipped: what is running (e.g. "claude.exe"). */
  skip?: string | null;
}

export interface LevelTotal {
  level: SelLevel;
  count: number;
  bytes: number;
}

export interface SelSummary {
  /** Picked items that can be selected (stale ids are ignored). */
  count: number;
  files: number;
  /** Bytes that will be freed: picked items that are not expected to be skipped. */
  bytes: number;
  /** Picked items expected to be skipped, and why (app names). */
  skipCount: number;
  skipBytes: number;
  skipReasons: string[];
  byLevel: LevelTotal[];
}

/** `true` when `item` may be ticked in this mode. */
export function selectable(item: SelItem, expert: boolean): boolean {
  if (item.locked) return false;
  const l = item.level;
  if (l === "protected" || l === "keep") return false;
  return l !== "advanced" || expert;
}

/** Picked ids that still exist and may be ticked (drops stale and forbidden ones). */
export function sanitize(items: SelItem[], picked: Iterable<string>, expert: boolean): string[] {
  const ok = new Set(items.filter((i) => selectable(i, expert)).map((i) => i.id));
  return [...new Set(picked)].filter((id) => ok.has(id));
}

/** Totals of the picked items: count, files, honest "will free" bytes, skips, per level. */
export function summarize(items: SelItem[], picked: Iterable<string>, expert: boolean): SelSummary {
  const set = new Set(picked);
  const out: SelSummary = { count: 0, files: 0, bytes: 0, skipCount: 0, skipBytes: 0, skipReasons: [], byLevel: [] };
  const levels = new Map<SelLevel, LevelTotal>();
  const reasons = new Set<string>();
  for (const i of items) {
    if (!set.has(i.id) || !selectable(i, expert)) continue;
    out.count++;
    out.files += i.files ?? 0;
    if (i.skip) {
      out.skipCount++;
      out.skipBytes += i.bytes;
      for (const r of i.skip.split(/,\s*/)) if (r) reasons.add(r);
    } else {
      out.bytes += i.bytes;
    }
    if (i.level) {
      const t = levels.get(i.level) ?? { level: i.level, count: 0, bytes: 0 };
      t.count++;
      t.bytes += i.bytes;
      levels.set(i.level, t);
    }
  }
  out.skipReasons = [...reasons].sort();
  out.byLevel = PICK_LEVELS.map((l) => levels.get(l)).filter((t): t is LevelTotal => !!t);
  return out;
}

/** Every selectable item. */
export function selectAll(items: SelItem[], expert: boolean): string[] {
  return items.filter((i) => selectable(i, expert)).map((i) => i.id);
}

/** Only the selectable items of `level` (replaces the selection). */
export function selectLevel(items: SelItem[], level: PickLevel, expert: boolean): string[] {
  return items.filter((i) => i.level === level && selectable(i, expert)).map((i) => i.id);
}

/** Levels with at least one selectable item, in display order. */
export function levelsPresent(items: SelItem[], expert: boolean): PickLevel[] {
  return PICK_LEVELS.filter((l) => items.some((i) => i.level === l && selectable(i, expert)));
}

export type GroupState = "all" | "some" | "none" | "empty";

/** How much of `group` is picked (`empty` = nothing in it can be ticked). */
export function groupState(items: SelItem[], group: string, picked: Iterable<string>, expert: boolean): GroupState {
  const set = new Set(picked);
  const mine = items.filter((i) => i.group === group && selectable(i, expert));
  if (!mine.length) return "empty";
  const n = mine.filter((i) => set.has(i.id)).length;
  return n === 0 ? "none" : n === mine.length ? "all" : "some";
}

/** Tick (`on`) or untick every selectable item of `group`, keeping the rest. */
export function setGroup(items: SelItem[], group: string, picked: string[], on: boolean, expert: boolean): string[] {
  const mine = new Set(items.filter((i) => i.group === group && selectable(i, expert)).map((i) => i.id));
  const rest = picked.filter((id) => !mine.has(id));
  return on ? [...rest, ...mine] : rest;
}

/** Toggle one id (ignored when it may not be ticked). */
export function toggle(items: SelItem[], picked: string[], id: string, expert: boolean): string[] {
  if (picked.includes(id)) return picked.filter((x) => x !== id);
  const item = items.find((i) => i.id === id);
  return item && selectable(item, expert) ? [...picked, id] : picked;
}

/** `true` when the confirmation must be acknowledged: Caution / Advanced items picked. */
export function needsAck(s: SelSummary): boolean {
  return s.byLevel.some((l) => l.level === "caution" || l.level === "advanced");
}

/** Should Ctrl+A / Esc in `target` go to the selection scope `scope`? Only when the
 *  key comes from inside that scope (and not a nested one, a text field or an open
 *  dialog) — or from the page body when `primary`. */
export function keyForScope(target: EventTarget | null, scope: Element | null | undefined, primary: boolean, doc: Document): boolean {
  if (!scope || doc.querySelector("[role=alertdialog]")) return false;
  const el = target instanceof Element ? target : null;
  if (!el || el === doc.body || el === doc.documentElement) return primary;
  if (el.closest("input:not([type=checkbox]):not([type=radio]), textarea, select, [contenteditable=true]")) return false;
  return el.closest("[data-selection-scope]") === scope;
}
