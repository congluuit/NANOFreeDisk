// Application state shared by the views (Svelte 5 runes).

import { api, errorText, on } from "./api";
import { destinationFor } from "./clean/destination";
import { i18n, systemLocale, t, type Locale } from "./i18n.svelte";
import { DEFAULT_THEME, type ThemeSettings } from "./theme";
import type { AppInfo, Filter, Finished, OldLargeOptions, Progress, SearchRows, Summary, Volume } from "./types";

export type View = "home" | "analyze" | "oldlarge" | "tidy" | "clean" | "activity" | "hardware" | "devcache" | "duplicates" | "monitor" | "health" | "programs" | "startup" | "settings" | "quick" | "tools";
export type Viz = "treemap" | "sunburst";
/** Analyze → Explorer: the map (`viz`) or the folder list. */
export type ExplorerLens = "map" | "folders";
/** G2: Simple hides advanced filters, raw attributes and technical exports; Expert
 * shows everything plus the equivalent `nfd` command for actions that have one. */
export type Mode = "simple" | "expert";

export interface Settings {
  locale: "system" | Locale;
  theme: ThemeSettings;
  fontScale: number;
  viz: Viz;
  /** Analyze → Explorer opens on the map or the folder list. */
  explorer: ExplorerLens;
  /** Treemap / sunburst colors: by file type or by verdict. */
  mapColor: "type" | "verdict";
  mode: Mode;
  /** Old & Large thresholds (A2). */
  largeGb: number;
  staleYears: number;
  staleMinMb: number;
  installerDays: number;
  /** Keep a snapshot of every scan for "What changed?" and the Health Score trend. */
  history: boolean;
  /** Hardware: notify when the CPU / GPU / a disk gets too hot (samples every 5 s). */
  tempAlerts: boolean;
  /** Start Advanced sensors (PawnIO helper, one UAC prompt) when the app opens. */
  advancedAuto: boolean;
  /** DevCache: remind when a vendor's caches pass this many GB (0 = never). */
  devThresholds: Record<string, number>;
  /** DevCache: system notification when a vendor is over its threshold. */
  devAlerts: boolean;
  /** DevCache → Repo: folder searched for repositories. */
  repoRoot: string;
  /** Build output of projects active in the last N days is not pre-selected (C9). */
  keepRecentDays: number;
  /** Projects without activity for N days are flagged stale. */
  staleDays: number;
  /** Duplicates: files smaller than this many MB are left out. */
  /** Smallest file the Duplicates search compares, in KiB. */
  dupeMinKb: number;
  /** Duplicates: how the copy to keep is chosen (auto, oldest, newest, shortest). */
  dupeKeep: string;
  /** Monitor: notify when a process keeps writing faster than `monitorMbps` MB/s
   * for `monitorMinutes` minutes (log-spam detector, C8). */
  monitorAlerts: boolean;
  monitorMbps: number;
  monitorMinutes: number;
  /** Drive health: notify when a drive's verdict gets worse, a drive gets too hot or
   * a volume drops below 10 % free (checked every 30 minutes while the app runs). */
  healthAlerts: boolean;
  /** The first-run welcome was closed (shown once). */
  welcomed: boolean;
  /** The last Quick clean: when it finished (Unix seconds) and what it freed. */
  lastQuick: { at: number; freed: number } | null;
}

export const DEFAULT_SETTINGS: Settings = {
  locale: "system",
  theme: DEFAULT_THEME,
  fontScale: 1,
  viz: "treemap",
  explorer: "map",
  mapColor: "type",
  mode: "simple",
  largeGb: 1,
  staleYears: 1,
  staleMinMb: 100,
  installerDays: 30,
  history: true,
  tempAlerts: false,
  advancedAuto: false,
  devThresholds: { Ollama: 50, "Hugging Face": 30, JetBrains: 10 },
  devAlerts: false,
  repoRoot: "",
  keepRecentDays: 7,
  staleDays: 90,
  dupeMinKb: 100,
  dupeKeep: "auto",
  monitorAlerts: false,
  monitorMbps: 5,
  monitorMinutes: 5,
  healthAlerts: true,
  welcomed: false,
  lastQuick: null,
};

interface Toast {
  id: number;
  text: string;
  kind: "ok" | "error" | "info";
}

export const app = $state({
  view: "home" as View,
  info: null as AppInfo | null,
  volumes: [] as Volume[],
  /** Scan in progress. */
  scanning: null as null | { id: number; path: string; progress: Progress | null },
  lastScanPath: "",
  summary: null as Summary | null,
  /** History snapshot of the loaded scan (null: not recorded). */
  snapshot: null as number | null,
  /** Directory shown in Analyze. */
  current: 0,
  /** Selected entry ids (multi-select). */
  selected: [] as number[],
  /** Entry shown in the details panel. */
  focus: null as number | null,
  /** Bumped after deletions and rescans so views refetch. */
  version: 0,
  search: null as null | { filter: Filter; result: SearchRows },
  deleting: null as null | number[],
  /** How the delete dialog removes `deleting` (`destinationFor`: the quarantine in
   * Simple mode) and which Cleanup history label it gets. */
  deleteMode: { mode: "recycle-bin", kind: "manual" } as { mode: "recycle-bin" | "quarantine"; kind: "manual" | "tidy" },
  /** Command palette (Ctrl+K) open. */
  palette: false,
  toasts: [] as Toast[],
  settings: structuredClone(DEFAULT_SETTINGS) as Settings,
  settingsLoaded: false,
});

let toastSeq = 0;
/** While above 0, "ok" and "info" toasts are dropped (Quick clean runs several tools
 * and shows one result screen instead); errors always show. */
let mutedToasts = 0;
export function muteToasts(on: boolean): void {
  mutedToasts = Math.max(0, mutedToasts + (on ? 1 : -1));
}

export function toast(text: string, kind: Toast["kind"] = "info"): void {
  if (mutedToasts > 0 && kind !== "error") return;
  const id = ++toastSeq;
  app.toasts.push({ id, text, kind });
  setTimeout(() => {
    app.toasts = app.toasts.filter((x) => x.id !== id);
  }, kind === "error" ? 8000 : 4500);
}

export function fail(e: unknown): void {
  const code = e && typeof e === "object" && "code" in e ? String((e as { code: unknown }).code) : "";
  toast(code === "no-session" ? t("error-no-session") : t("error-generic", { message: errorText(e) }), "error");
}

export const expert = (): boolean => app.settings.mode === "expert";

const DAY = 86_400;

/** Old & Large thresholds from the settings, relative to now. */
export function oldLargeOptions(limit = 300): OldLargeOptions {
  const now = Math.floor(Date.now() / 1000);
  const s = app.settings;
  return {
    large_min: Math.round(s.largeGb * 2 ** 30),
    stale_min: Math.round(s.staleMinMb * 2 ** 20),
    stale_before: now - Math.round(s.staleYears * 365 * DAY),
    installer_before: now - s.installerDays * DAY,
    limit,
  };
}

export function applyLocale(): void {
  i18n.locale = app.settings.locale === "system" ? systemLocale() : app.settings.locale;
  document.documentElement.lang = i18n.locale;
}

export async function loadSettings(): Promise<void> {
  try {
    const raw = (await api.loadSettings()) as Partial<Settings> | null;
    if (raw && typeof raw === "object") {
      app.settings = {
        ...DEFAULT_SETTINGS,
        ...raw,
        theme: { ...DEFAULT_THEME, ...(raw.theme ?? {}), overrides: { ...(raw.theme?.overrides ?? {}) } },
      };
    }
  } catch (e) {
    console.warn("settings", e);
  }
  app.settingsLoaded = true;
  // automatic cleaning / low space alert texts and the tray's "Free up space"
  // (docs/SCHEDULE.md); imported late to keep state.svelte free of a cycle
  void import("./schedule/watch.svelte").then((m) => m.startScheduleWatch()).catch(() => {});
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;
export function saveSettingsSoon(): void {
  clearTimeout(saveTimer);
  const snapshot = $state.snapshot(app.settings);
  saveTimer = setTimeout(() => api.saveSettings(snapshot).catch(fail), 400);
}

export async function refreshVolumes(): Promise<void> {
  try {
    app.volumes = await api.listVolumes();
  } catch (e) {
    fail(e);
  }
}

let listening = false;
export async function listenScanEvents(): Promise<void> {
  if (listening) return;
  listening = true;
  // A7: "Analyze with NANOFreeDisk" on a folder while the app is already open
  await on<string>("open-path", (path) => {
    if (path) void startScan(path);
  });
  await on<Progress>("scan-progress", (p) => {
    if (app.scanning && p.scan === app.scanning.id) app.scanning.progress = p;
  });
  await on<Finished>("scan-done", async (f) => {
    // Ignore a superseded scan while another one is tracked; otherwise accept it,
    // even if this window did not start it (rescan from elsewhere, automation).
    if (app.scanning && app.scanning.id !== -1 && f.scan !== app.scanning.id) return;
    if (!app.scanning && !f.ok) return;
    app.scanning = null;
    const back = scanReturnView;
    scanReturnView = null;
    if (f.ok) {
      app.snapshot = f.snapshot ?? null;
      if (f.historyError) toast(t("history-save-failed", { error: f.historyError }), "error");
      await loadSummary();
      app.current = app.summary?.rootId ?? 0;
      app.selected = [];
      app.focus = app.current;
      app.search = null;
      app.version++;
      if (back !== "stay") app.view = back ?? "analyze";
    } else if (f.cancelled) {
      toast(t("scan-cancelled"));
    } else {
      toast(t("scan-failed", { error: f.error ?? "?" }), "error");
    }
    settleScanWaiters(f);
  });
}

/** Callers waiting for the tracked scan to end (Quick clean). */
let scanWaiters: ((f: Finished) => void)[] = [];
function settleScanWaiters(f: Finished): void {
  const waiting = scanWaiters;
  scanWaiters = [];
  for (const done of waiting) done(f);
}

/** Resolves when the scan being tracked ends (after its summary is loaded), or
 * failed to start. Call it before `startScan` so the end is never missed. */
export function waitScan(): Promise<Finished> {
  return new Promise((resolve) => scanWaiters.push(resolve));
}

/** A7: the folder the app was started with (Explorer menu, command line). */
export async function openStartupPath(): Promise<void> {
  try {
    const path = await api.takeStartupPath();
    if (path) await startScan(path);
  } catch (e) {
    fail(e);
  }
}

export async function loadSummary(): Promise<void> {
  try {
    app.summary = await api.summary();
  } catch (e) {
    app.summary = null;
    fail(e);
  }
}

/** The view a finished scan opens instead of Analyze (Tidy up's "Scan drive C:"), or
 * "stay" to leave the user where they are (Quick clean). */
let scanReturnView: View | "stay" | null = null;

export async function startScan(path: string, returnTo?: View | "stay"): Promise<void> {
  scanReturnView = returnTo ?? null;
  try {
    await listenScanEvents();
    app.lastScanPath = path;
    app.scanning = { id: -1, path, progress: null };
    const id = await api.startScan(path, app.settings.history);
    if (app.scanning) app.scanning.id = id;
  } catch (e) {
    app.scanning = null;
    fail(e);
    settleScanWaiters({ scan: -1, ok: false, cancelled: false, error: String((e as { message?: unknown })?.message ?? e), snapshot: null, historyError: null });
  }
}

export function navigate(id: number): void {
  app.current = id;
  app.focus = id;
  app.selected = [];
}

/** Select `id`; with `additive` toggle it in the multi-selection. */
export function select(id: number, additive = false): void {
  if (additive) {
    app.selected = app.selected.includes(id) ? app.selected.filter((x) => x !== id) : [...app.selected, id];
  } else {
    app.selected = [id];
  }
  app.focus = id;
}

/** Ask before deleting hand-picked entries (DeleteDialog). Where they go follows the
 *  one rule of `destinationFor`: the quarantine in Simple mode, the tool's own default
 *  in Expert mode (Recycle Bin for Analyze / Old & large, quarantine for Tidy up). */
export function askDelete(ids: number[], kind: "manual" | "tidy" = "manual"): void {
  const list = ids.filter((id) => id !== (app.summary?.rootId ?? 0));
  if (!list.length) return;
  app.deleteMode = { mode: destinationFor(kind, expert()), kind };
  app.deleting = list;
}

/** After a deletion: refresh totals, drop removed ids, let views refetch. */
export async function afterDelete(removed: number[]): Promise<void> {
  app.selected = app.selected.filter((x) => !removed.includes(x));
  if (app.focus != null && removed.includes(app.focus)) app.focus = app.current;
  if (removed.includes(app.current)) app.current = app.summary?.rootId ?? 0;
  await loadSummary();
  if (app.search) {
    try {
      app.search.result = await api.search(app.current, app.search.filter, 500);
    } catch (e) {
      fail(e);
    }
  }
  app.version++;
}
