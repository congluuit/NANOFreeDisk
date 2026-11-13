// "Similar photos" state: which half of the Duplicates view is shown, where to look,
// the last search, what the user ticked (never anything by default) and the
// running removal.

import { on } from "../api";
import type { CleanProgress, OperationResult } from "../clean/types";
import { formatSize } from "../format";
import { intlLocale, t } from "../i18n.svelte";
import { fail, toast } from "../state.svelte";
import { similarApi } from "./api";
import { allButBest, prunePicks, selections, setGroupPicks, togglePick, type Picks } from "./logic";
import type { DupesMode, Preset, SimilarAction, SimilarProgress, SimilarReport, SimilarScope } from "./types";

export const similar = $state({
  mode: "identical" as DupesMode,
  scope: "personal" as SimilarScope,
  folders: [] as string[],
  preset: "strict" as Preset,
  /** Expert: also list application folders (shown only). */
  includeAppDirs: false,
  report: null as SimilarReport | null,
  /** Search running (progress of the current step). */
  searching: null as SimilarProgress | null,
  /** Where the last search looked (what "search again" repeats). */
  lastScope: null as null | { scope: SimilarScope; folders: string[] },
  /** Per group id: the photos ticked for removal. Empty after every search. */
  picks: {} as Picks,
  /** Groups rendered so far (the list grows on demand). */
  shown: 30,
  confirm: null as null | SimilarAction,
  running: null as CleanProgress | null,
  last: null as OperationResult | null,
  thumbs: {} as Record<string, string | null>,
  /** Side-by-side view: the group and the photo shown next to its best one. */
  compare: null as null | { group: number; path: string },
});

let progressOff: (() => void) | null = null;
let cleanOff: (() => void) | null = null;

/** Test hook: the GUI smoke test keeps its fixture out of the cache (as Duplicates). */
const useCache = (): boolean => !(globalThis as { __nfdNoDupeCache?: boolean }).__nfdNoDupeCache;

function codeOf(e: unknown): string {
  return e && typeof e === "object" && "code" in e ? String((e as { code: unknown }).code) : "";
}

/** Search the chosen scope. Nothing is ticked afterwards. */
export async function findSimilar(scope: SimilarScope = similar.scope): Promise<void> {
  if (similar.searching) return;
  similar.scope = scope;
  const paths = scope === "folders" ? [...similar.folders] : [];
  if (scope === "folders" && !paths.length) return;
  similar.searching = { phase: scope === "current" ? "collect" : "scan", done: 0, total: 0, path: paths[0] ?? "" };
  similar.lastScope = { scope, folders: paths };
  similar.last = null;
  try {
    progressOff ??= await on<SimilarProgress>("similar-progress", (p) => {
      if (similar.searching) similar.searching = p;
    });
    const report = await similarApi.find(paths, {
      preset: similar.preset,
      includeAppDirs: similar.includeAppDirs,
      useCache: useCache(),
      personal: scope === "personal",
    });
    similar.report = report;
    similar.picks = {};
    similar.shown = 30;
    similar.thumbs = {};
    waiting.length = 0;
    similar.compare = null;
  } catch (e) {
    if (codeOf(e) === "cancelled") toast(t("similar-cancelled"));
    else fail(e);
  } finally {
    similar.searching = null;
  }
}

/** Group the last search again for `preset` (no file is read again). */
export async function changePreset(preset: Preset): Promise<void> {
  similar.preset = preset;
  if (!similar.report || similar.searching) return;
  similar.searching = { phase: "compare", done: 0, total: similar.report.images, path: "" };
  try {
    progressOff ??= await on<SimilarProgress>("similar-progress", (p) => {
      if (similar.searching) similar.searching = p;
    });
    const report = await similarApi.regroup(preset);
    similar.report = report;
    // other groups, other ids: start again from nothing ticked
    similar.picks = {};
    similar.shown = 30;
    similar.compare = null;
  } catch (e) {
    if (codeOf(e) === "cancelled") toast(t("similar-cancelled"));
    else fail(e);
  } finally {
    similar.searching = null;
  }
}

export function togglePhoto(group: number, path: string): void {
  similar.picks = togglePick(similar.picks, group, path);
}

/** "Select all but the best" of one group (risky photos stay unticked). */
export function pickAllButBest(group: number): void {
  const g = similar.report?.groups.find((x) => x.id === group);
  if (g) similar.picks = setGroupPicks(similar.picks, group, allButBest(g));
}

export function clearGroup(group: number): void {
  similar.picks = setGroupPicks(similar.picks, group, []);
}

/** The "remove all photos of this group" tick. */
export function setKeepNone(group: number, on: boolean): void {
  const p = similar.picks[group];
  if (p) similar.picks = { ...similar.picks, [group]: { ...p, keepNone: on } };
}

export function clearPicks(): void {
  similar.picks = {};
}

/** Remove the ticked photos (already confirmed). */
export async function runSimilar(action: SimilarAction): Promise<void> {
  const groups = similar.report?.groups ?? [];
  const { ok } = selections(groups, similar.picks);
  if (similar.running || !ok.length) return;
  similar.running = { done: 0, total: 0, freed: 0, path: "" };
  similar.last = null;
  try {
    cleanOff ??= await on<CleanProgress>("clean-progress", (p) => {
      if (similar.running) similar.running = p;
    });
    const result = await similarApi.run(ok, action);
    similar.last = result;
    toast(
      t("clean-result", { size: formatSize(result.freed, intlLocale()), done: result.done + result.partial, skipped: result.skipped + result.failed }),
      result.failed ? "error" : "ok",
    );
    if (result.journalError) toast(t("clean-journal-failed", { error: result.journalError }), "error");
  } catch (e) {
    fail(e);
  } finally {
    similar.running = null;
    try {
      const fresh = await similarApi.report();
      similar.report = fresh;
      similar.picks = fresh ? prunePicks(fresh.groups, similar.picks) : {};
      similar.compare = null;
    } catch (e) {
      fail(e);
    }
  }
}

// grid previews are decoded on the backend: two at a time, in the order asked
const waiting: string[] = [];
let busy = 0;

async function pump(): Promise<void> {
  while (busy < 2 && waiting.length) {
    const path = waiting.shift()!;
    busy++;
    try {
      similar.thumbs[path] = await similarApi.thumbnail(path);
    } catch {
      similar.thumbs[path] = null;
    } finally {
      busy--;
    }
  }
}

/** Grid preview of a photo (loaded once). */
export function loadThumb(path: string): void {
  if (path in similar.thumbs) return;
  similar.thumbs[path] = null;
  waiting.push(path);
  void pump();
}

/** The large pictures of the side-by-side view (the last pair only). */
export const large = $state({ paths: [] as string[], urls: {} as Record<string, string | null> });

export async function loadLarge(paths: string[]): Promise<void> {
  large.paths = paths;
  large.urls = {};
  await Promise.all(
    paths.map(async (p) => {
      try {
        const url = await similarApi.thumbnail(p, true);
        if (large.paths.includes(p)) large.urls[p] = url;
      } catch {
        large.urls[p] = null;
      }
    }),
  );
}
