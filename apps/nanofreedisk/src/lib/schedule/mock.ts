// Browser-only stand-in for automatic cleaning (`pnpm dev` without Tauri). "Run
// now" adds a `clean-scheduled` entry to the mock Cleanup history through the
// recorder the main mock hands in; nothing touches a real disk or registry.

import { DEFAULT_CONFIG, sanitize } from "./model.ts";
import type { PreviewRule, ScheduleConfig, SchedulePreview, ScheduleState } from "./types";

type Recorder = (kind: string, items: { path: string; bytes: number; isDir: boolean }[], mode: "quarantine") => void;

let recorder: Recorder = () => {};
/** The main mock passes the Cleanup history recorder (`mockDeleted`). */
export function setScheduleRecorder(fn: Recorder): void {
  recorder = fn;
}

const CATEGORIES = ["browser", "gpu", "package-manager", "system"];

const RULES: (PreviewRule & { path: string })[] = [
  { ruleId: "windows.temp.user", app: "Windows", category: "system", bytes: 1.8e9, items: 412, path: "C:\\Users\\nano\\AppData\\Local\\Temp" },
  { ruleId: "browser.chrome.cache", app: "Google Chrome", category: "browser", bytes: 950e6, items: 3, path: "C:\\Users\\nano\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache" },
  { ruleId: "gpu.nvidia.dxcache", app: "NVIDIA", category: "gpu", bytes: 420e6, items: 1, path: "C:\\Users\\nano\\AppData\\Local\\NVIDIA\\DXCache" },
  { ruleId: "npm.cache", app: "npm", category: "package-manager", bytes: 2.4e9, items: 1, path: "C:\\Users\\nano\\AppData\\Local\\npm-cache" },
];

let config: ScheduleConfig = { ...DEFAULT_CONFIG };
let state: Omit<ScheduleState, "config" | "categories" | "supported"> = {
  last: null,
  next: null,
  autostart: { supported: true, on: false, blocked: false },
  trayStart: false,
};
let enabledAt: number | null = null;
let previewed = false;

export function resetScheduleMock(): void {
  config = { ...DEFAULT_CONFIG };
  state = { last: null, next: null, autostart: { supported: true, on: false, blocked: false }, trayStart: false };
  enabledAt = null;
  previewed = false;
}

const now = (): number => Math.floor(Date.now() / 1000);
const DAY = 86_400;

function snapshot(): ScheduleState {
  const every = config.frequency === "weekly" ? 7 * DAY : config.frequency === "monthly" ? 30 * DAY : null;
  const base = state.last?.at ?? enabledAt;
  return {
    ...state,
    config: { ...config, offCategories: [...config.offCategories] },
    supported: true,
    categories: [...CATEGORIES],
    next: every && base != null ? base + every : null,
  };
}

function picked(): (PreviewRule & { path: string })[] {
  return RULES.filter((r) => !config.offCategories.includes(r.category));
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const scheduleHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  schedule_get: () => snapshot(),
  schedule_set: ({ config: c }) => {
    config = sanitize(c as ScheduleConfig);
    if (config.frequency === "off") enabledAt = null;
    else enabledAt ??= now();
    previewed = false;
    return snapshot();
  },
  schedule_texts: () => undefined,
  schedule_preview: (): SchedulePreview => {
    previewed = true;
    const rules: PreviewRule[] = picked().map((r) => ({ ruleId: r.ruleId, app: r.app, category: r.category, bytes: r.bytes, items: r.items }));
    return { rules, bytes: rules.reduce((s, r) => s + r.bytes, 0), items: rules.reduce((s, r) => s + r.items, 0) };
  },
  schedule_run_now: () => {
    if (!previewed) throw { code: "no-preview", message: "show the preview again" };
    previewed = false;
    const rules = picked();
    if (!rules.length) throw { code: "nothing-to-clean", message: "the rules have nothing to clean" };
    recorder("clean-scheduled", rules.map((r) => ({ path: r.path, bytes: r.bytes, isDir: true })), "quarantine");
    const freed = rules.reduce((s, r) => s + r.bytes, 0);
    state.last = { at: now(), freed, items: rules.length, op: `mock-${now()}` };
    return { id: state.last.op, freed, done: rules.length, partial: 0, skipped: 0, failed: 0, cancelled: false, problems: [], journalError: null };
  },
  autostart_set: ({ enabled }) => {
    state.autostart = { ...state.autostart, on: Boolean(enabled), blocked: false };
    return { ...state.autostart };
  },
};
