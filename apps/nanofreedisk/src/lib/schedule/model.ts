// Pure helpers of the automatic cleaning settings (no runes, tested in
// schedule.test.ts).

import type { ScheduleConfig } from "./types";

type Translate = (id: string, args?: Record<string, string | number>) => string;

/** Backend text key to locale key and the placeholders the backend fills in. */
export const NOTE_KEYS: Record<string, [string, Record<string, string>]> = {
  "auto-title": ["schedule-note-auto-title", {}],
  "auto-body": ["schedule-note-auto-body", { size: "{size}" }],
  "low-title": ["schedule-note-low-title", { drive: "{drive}" }],
  "low-body": ["schedule-note-low-body", { drive: "{drive}", free: "{free}" }],
  "tray-free-up": ["schedule-tray-free-up", {}],
};

/** Notification and tray texts in the UI language, for the backend. */
export function noteTexts(t: Translate): Record<string, string> {
  return Object.fromEntries(Object.entries(NOTE_KEYS).map(([k, [id, args]]) => [k, t(id, args)]));
}

/** The view "Free up space" opens: Quick clean when the app has it, else Clean,
 *  else Home (macOS has no Clean view). */
export function freeUpView(hasQuick: boolean, hasClean: boolean): "quick" | "clean" | "home" {
  if (hasQuick) return "quick";
  return hasClean ? "clean" : "home";
}

/** Label of a rule category, or the category itself when there is none. */
export function categoryLabel(category: string, t: Translate): string {
  const key = `schedule-cat-${category}`;
  const text = t(key);
  return text === key ? category : text;
}

/** The left-out list after ticking (`on`) or unticking a category. */
export function setCategory(off: string[], category: string, on: boolean): string[] {
  const rest = off.filter((c) => c !== category);
  return on ? rest : [...rest, category];
}

const clamp = (v: number, lo: number, hi: number, def: number): number =>
  Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : def;

/** The config with numbers in the backend's ranges. */
export function sanitize(c: ScheduleConfig): ScheduleConfig {
  return {
    ...c,
    lowPercent: clamp(c.lowPercent, 1, 50, 10),
    lowGb: clamp(c.lowGb, 1, 10_000, 10),
    offCategories: c.offCategories.filter((x) => x !== ""),
  };
}

export const DEFAULT_CONFIG: ScheduleConfig = {
  frequency: "off",
  idleOnly: true,
  offCategories: [],
  lowSpace: false,
  lowPercent: 10,
  lowGb: 10,
};
