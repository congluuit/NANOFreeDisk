// Duplicates view state (PHASE5_DUPLICATES §2.6): scope, the last search, the copy
// kept and the copies removed per group, and the running removal.

import { on } from "../api";
import type { CleanProgress, OperationResult } from "../clean/types";
import { formatSize } from "../format";
import { intlLocale, t } from "../i18n.svelte";
import { app, fail, toast } from "../state.svelte";
import { dupesApi } from "./api";
import type { DupeAction, DupeGroup, DupeProgress, DupeReport, FileCopy, FileKind, FolderCopy, FolderGroup, KeepStrategy, LinkKind, Scope, Selection } from "./types";

/** Extensions of each file-kind filter (lower case, no dot). */
export const KINDS: Record<Exclude<FileKind, "all">, string[]> = {
  images: ["jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif", "tif", "tiff", "dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2"],
  videos: ["mp4", "mkv", "mov", "avi", "wmv", "webm", "m4v", "mts", "m2ts", "3gp"],
  music: ["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma"],
  documents: ["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf", "txt", "epub"],
  archives: ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "img", "cab"],
};

/** Hard-linked without asking: rarely edited in place (mirror of `nfd_dupes::plan::LINK_EXTS`). */
export const LINK_EXTS = new Set([
  "jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif", "tif", "tiff", "dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2",
  "mp4", "mkv", "mov", "avi", "wmv", "webm", "m4v", "mts", "m2ts", "3gp", "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma",
  "exe", "msi", "msix", "appx", "iso", "img", "vhd", "vhdx", "zip", "7z", "rar", "tar", "gz", "xz", "bz2", "zst", "cab",
  "gguf", "safetensors", "ckpt", "onnx", "pt", "pth",
]);

/** How `copy` can become a link to `keeper`, or why not (mirror of `can_link`). */
export function canLink(keeper: FileCopy, copy: FileCopy): LinkKind | string {
  if (keeper.volume !== copy.volume) return "other-volume";
  if (!copy.link) return "no-link-support";
  if (keeper.flags.cloud || copy.flags.cloud) return "cloud-sync";
  if (keeper.flags.git || copy.flags.git) return "git";
  if (keeper.flags.appDir || copy.flags.appDir) return "app-folder";
  return copy.link;
}

const extOf = (p: string) => (/\.([^.\\/]+)$/.exec(p)?.[1] ?? "").toLowerCase();

/** The selected copies a "replace with links" carries out, and what stays as is. */
export function linkPlan(otherTypes: boolean): { selections: Selection[]; files: number; bytes: number; kinds: Set<LinkKind>; skipped: number; otherTypes: number } {
  const selections: Selection[] = [];
  const kinds = new Set<LinkKind>();
  let files = 0;
  let bytes = 0;
  let skipped = 0;
  let others = 0;
  for (const g of dupes.report?.groups ?? []) {
    const p = dupes.picks[g.id];
    if (!p?.remove.length) continue;
    const keeper = g.copies.find((c) => c.path === p.keep);
    if (!keeper) continue;
    const remove: string[] = [];
    for (const c of g.copies.filter((x) => p.remove.includes(x.path))) {
      const k = canLink(keeper, c);
      if (k !== "hardlink" && k !== "clone") {
        skipped++;
      } else if (k === "hardlink" && !otherTypes && !LINK_EXTS.has(extOf(c.path))) {
        others++;
      } else {
        remove.push(c.path);
        kinds.add(k);
        files++;
        bytes += g.size;
      }
    }
    if (remove.length) selections.push({ group: g.id, keep: p.keep, remove });
  }
  return { selections, files, bytes, kinds, skipped, otherTypes: others };
}

export interface Pick {
  keep: string;
  remove: string[];
}

export const dupes = $state({
  scope: "current" as Scope,
  folders: [] as string[],
  kind: "all" as FileKind,
  /** Expert: also list application folders (never pre-selected). */
  includeAppDirs: false,
  /** Expert: compare bytes after hashing. */
  paranoid: false,
  /** Also look for duplicate folders (every file is hashed then). */
  findFolders: false,
  tab: "files" as "files" | "folders",
  /** Per folder group id: the folder kept and the folders to remove. */
  folderPicks: {} as Record<number, Pick>,
  report: null as DupeReport | null,
  /** Search running (progress of the current step). */
  searching: null as DupeProgress | null,
  strategy: "auto" as KeepStrategy,
  /** Per group id: the copy kept and the copies to remove. */
  picks: {} as Record<number, Pick>,
  /** Groups rendered so far (the list grows on demand). */
  shown: 60,
  expanded: [] as number[],
  confirm: null as null | DupeAction,
  running: null as CleanProgress | null,
  last: null as OperationResult | null,
  thumbs: {} as Record<string, string | null>,
});

/** Risky to remove: never pre-selected, extra confirmation. */
export function risky(c: FileCopy | FolderCopy): boolean {
  return c.flags.cloud || c.flags.git || c.flags.appDir || c.flags.readonly;
}

function toFolderPicks(sel: Selection[], groups: FolderGroup[]): Record<number, Pick> {
  const out: Record<number, Pick> = {};
  for (const g of groups) out[g.id] = { keep: g.folders.find((f) => f.blocked)?.path ?? g.folders[0]?.path ?? "", remove: [] };
  for (const s of sel) out[s.group] = { keep: s.keep, remove: [...s.remove] };
  return out;
}

/** Folder selections for the backend. */
export function folderSelections(): Selection[] {
  return Object.entries(dupes.folderPicks)
    .filter(([, p]) => p.remove.length > 0)
    .map(([id, p]) => ({ group: Number(id), keep: p.keep, remove: p.remove }));
}

/** Folders selected for removal, their size and the risky ones. */
export function folderTotals(): { folders: number; bytes: number; files: number; risky: FolderCopy[] } {
  let folders = 0;
  let bytes = 0;
  let files = 0;
  const flagged: FolderCopy[] = [];
  for (const g of dupes.report?.folderGroups ?? []) {
    const p = dupes.folderPicks[g.id];
    for (const f of g.folders) {
      if (!p?.remove.includes(f.path)) continue;
      folders++;
      bytes += g.size;
      files += g.files;
      if (risky(f)) flagged.push(f);
    }
  }
  return { folders, bytes, files, risky: flagged };
}

export function setFolderKeep(group: number, path: string): void {
  const p = dupes.folderPicks[group];
  if (!p) return;
  dupes.folderPicks[group] = { keep: path, remove: p.remove.filter((x) => x !== path) };
}

export function toggleFolderRemove(group: number, path: string): void {
  const p = dupes.folderPicks[group];
  if (!p || p.keep === path) return;
  dupes.folderPicks[group] = {
    keep: p.keep,
    remove: p.remove.includes(path) ? p.remove.filter((x) => x !== path) : [...p.remove, path],
  };
}

function toPicks(sel: Selection[], groups: DupeGroup[]): Record<number, Pick> {
  const out: Record<number, Pick> = {};
  for (const g of groups) out[g.id] = { keep: g.copies[0]?.path ?? "", remove: [] };
  for (const s of sel) out[s.group] = { keep: s.keep, remove: [...s.remove] };
  return out;
}

/** Selections for the backend: groups with something to remove. */
export function selections(): Selection[] {
  return Object.entries(dupes.picks)
    .filter(([, p]) => p.remove.length > 0)
    .map(([id, p]) => ({ group: Number(id), keep: p.keep, remove: p.remove }));
}

/** Copies (and their size) currently selected for removal. */
export function selectedTotals(): { files: number; bytes: number; groups: number; risky: FileCopy[] } {
  let files = 0;
  let bytes = 0;
  let groups = 0;
  const flagged: FileCopy[] = [];
  for (const g of dupes.report?.groups ?? []) {
    const p = dupes.picks[g.id];
    if (!p?.remove.length) continue;
    groups++;
    for (const c of g.copies) {
      if (!p.remove.includes(c.path)) continue;
      files++;
      bytes += g.size;
      if (risky(c)) flagged.push(c);
    }
  }
  return { files, bytes, groups, risky: flagged };
}

export function setKeep(group: number, path: string): void {
  const p = dupes.picks[group];
  if (!p) return;
  dupes.picks[group] = { keep: path, remove: p.remove.filter((x) => x !== path) };
}

export function toggleRemove(group: number, path: string): void {
  const p = dupes.picks[group];
  if (!p || p.keep === path) return;
  dupes.picks[group] = {
    keep: p.keep,
    remove: p.remove.includes(path) ? p.remove.filter((x) => x !== path) : [...p.remove, path],
  };
}

/** Re-pick every group with `strategy` (the backend applies the keep rules). */
export async function applyStrategy(strategy: KeepStrategy): Promise<void> {
  dupes.strategy = strategy;
  app.settings.dupeKeep = strategy;
  if (!dupes.report) return;
  try {
    dupes.picks = toPicks(await dupesApi.select(strategy), dupes.report.groups);
    dupes.folderPicks = toFolderPicks(await dupesApi.select(strategy, true), dupes.report.folderGroups ?? []);
  } catch (e) {
    fail(e);
  }
}

/** Selection id of a copy (file or folder) in its group. */
export const copyId = (group: number, path: string): string => `${group}|${path}`;

function pickIds(picks: Record<number, Pick>): string[] {
  return Object.entries(picks).flatMap(([id, p]) => p.remove.map((path) => copyId(Number(id), path)));
}

/** Rebuild per-group picks from selection ids; the copy each group keeps never goes. */
function fromIds(picks: Record<number, Pick>, ids: string[]): Record<number, Pick> {
  const out: Record<number, Pick> = {};
  for (const [id, p] of Object.entries(picks)) out[Number(id)] = { keep: p.keep, remove: [] };
  for (const sel of ids) {
    const i = sel.indexOf("|");
    const g = out[Number(sel.slice(0, i))];
    const path = sel.slice(i + 1);
    if (g && path !== g.keep && !g.remove.includes(path)) g.remove.push(path);
  }
  return out;
}

/** Selection ids of the file copies to remove. */
export const filePickIds = (): string[] => pickIds(dupes.picks);
export function setFilePicks(ids: string[]): void {
  dupes.picks = fromIds(dupes.picks, ids);
}
/** Selection ids of the folder copies to remove. */
export const folderPickIds = (): string[] => pickIds(dupes.folderPicks);
export function setFolderPicks(ids: string[]): void {
  dupes.folderPicks = fromIds(dupes.folderPicks, ids);
}

/** Remove nothing: keep every copy. */
export function clearPicks(): void {
  for (const id of Object.keys(dupes.picks)) dupes.picks[Number(id)].remove = [];
  for (const id of Object.keys(dupes.folderPicks)) dupes.folderPicks[Number(id)].remove = [];
}

let progressOff: (() => void) | null = null;
let cleanOff: (() => void) | null = null;

/** Extensions of the chosen kind (empty = every file). */
export function extensions(): string[] {
  return dupes.kind === "all" ? [] : KINDS[dupes.kind];
}

/** Test hook: the GUI smoke test keeps its fixture out of the hash cache. */
const useCache = (): boolean => !(globalThis as { __nfdNoDupeCache?: boolean }).__nfdNoDupeCache;

/** Search the chosen scope. */
export async function findDupes(): Promise<void> {
  if (dupes.searching) return;
  const paths = dupes.scope === "current" ? [] : [...dupes.folders];
  if (dupes.scope === "folders" && !paths.length) return;
  dupes.searching = { phase: dupes.scope === "current" ? "collect" : "scan", done: 0, total: 0, bytes: 0, totalBytes: 0, path: paths[0] ?? "" };
  dupes.last = null;
  try {
    progressOff ??= await on<DupeProgress>("dupes-progress", (p) => {
      if (dupes.searching) dupes.searching = p;
    });
    const report = await dupesApi.find(paths, {
      minSize: Math.max(1, Math.round(app.settings.dupeMinMb * 2 ** 20)),
      includeAppDirs: dupes.includeAppDirs,
      paranoid: dupes.paranoid,
      extensions: extensions(),
      useCache: useCache(),
      folders: dupes.findFolders,
    });
    dupes.report = report;
    dupes.shown = 60;
    dupes.expanded = report.groups.slice(0, 3).map((g) => g.id);
    dupes.thumbs = {};
    dupes.picks = toPicks(await dupesApi.select(dupes.strategy), report.groups);
    dupes.folderPicks = toFolderPicks(await dupesApi.select(dupes.strategy, true), report.folderGroups ?? []);
    if (!report.folderGroups?.length) dupes.tab = "files";
  } catch (e) {
    const code = e && typeof e === "object" && "code" in e ? String((e as { code: unknown }).code) : "";
    if (code === "cancelled") toast(t("dupes-cancelled"));
    else fail(e);
  } finally {
    dupes.searching = null;
  }
}

/** "Find duplicates here" from Analyze or Explorer. */
export function openDupesFor(path: string): void {
  app.view = "duplicates";
  dupes.scope = "folders";
  dupes.folders = [path];
  void findDupes();
}

/** The folder the app was started with through "Find duplicates here". */
export async function openStartupDupes(): Promise<void> {
  try {
    await on<string>("open-dupes", (path) => path && openDupesFor(path));
    const path = await dupesApi.takeStartup();
    if (path) openDupesFor(path);
  } catch (e) {
    fail(e);
  }
}

/** Remove (or replace by links) the selected copies or folders (already confirmed). */
export async function runDupes(action: DupeAction, only?: Selection[], folders = false): Promise<void> {
  const sel = only ?? (folders ? folderSelections() : selections());
  if (dupes.running || !sel.length) return;
  dupes.running = { done: 0, total: 0, freed: 0, path: "" };
  dupes.last = null;
  try {
    cleanOff ??= await on<CleanProgress>("clean-progress", (p) => {
      if (dupes.running) dupes.running = p;
    });
    const result = await dupesApi.run(sel, action, folders);
    dupes.last = result;
    toast(
      t("clean-result", { size: formatSize(result.freed, intlLocale()), done: result.done + result.partial, skipped: result.skipped + result.failed }),
      result.failed ? "error" : "ok",
    );
    if (result.journalError) toast(t("clean-journal-failed", { error: result.journalError }), "error");
  } catch (e) {
    fail(e);
  } finally {
    dupes.running = null;
    app.version++;
    try {
      const fresh = await dupesApi.report();
      dupes.report = fresh;
      if (fresh) {
        dupes.picks = toPicks(await dupesApi.select(dupes.strategy), fresh.groups);
        dupes.folderPicks = toFolderPicks(await dupesApi.select(dupes.strategy, true), fresh.folderGroups ?? []);
      }
    } catch (e) {
      fail(e);
    }
  }
}

/** Preview of an image copy (loaded once). */
export async function loadThumb(path: string): Promise<void> {
  if (path in dupes.thumbs) return;
  dupes.thumbs[path] = null;
  try {
    dupes.thumbs[path] = await dupesApi.thumbnail(path);
  } catch {
    dupes.thumbs[path] = null;
  }
}

/** The equivalent `nfd dupes` command (Expert mode). */
export function cliCommand(action?: DupeAction): string {
  const paths = dupes.scope === "current" ? dupes.report?.roots ?? [] : dupes.folders;
  const parts = ["nfd dupes", ...paths.map((p) => `"${p}"`), `--min-size ${app.settings.dupeMinMb}MB`];
  const ext = extensions();
  if (ext.length) parts.push(`--ext ${ext.join(",")}`);
  if (dupes.includeAppDirs) parts.push("--include-app-dirs");
  if (dupes.paranoid) parts.push("--paranoid");
  if (dupes.tab === "folders") parts.push("--folders");
  parts.push(`--keep ${dupes.strategy}`);
  if (action) parts.push(`--action ${action === "link" ? "hardlink" : action}`, "--apply");
  return parts.join(" ");
}
