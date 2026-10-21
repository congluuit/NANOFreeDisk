// Typed bridge to the Rust commands. Outside Tauri (plain `vite` in a browser) the
// calls go to an in-memory mock so the UI can be developed and previewed without a
// native build.

import type {
  AdviceOutcome,
  AdviceSummary,
  EntryAdvice,
  UpdateInfo,
  AppInfo,
  AgeMatrix,
  Article,
  DeletePlan,
  Detail,
  Diff,
  EmptyRows,
  ExtRow,
  Filter,
  Finished,
  HealthScore,
  HistoryInfo,
  Located,
  Node,
  OldLargeOptions,
  OldLargeRows,
  Progress,
  RecycleOutcome,
  ScanIssue,
  SearchRows,
  ShortcutRows,
  Slice,
  Snapshot,
  Summary,
  Volume,
} from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
type Listen = <T>(event: string, cb: (payload: T) => void) => Promise<() => void>;

let invokeImpl: Invoke | undefined;
let listenImpl: Listen | undefined;

async function backend(): Promise<void> {
  if (invokeImpl) return;
  if (inTauri) {
    const core = await import("@tauri-apps/api/core");
    const event = await import("@tauri-apps/api/event");
    invokeImpl = core.invoke;
    listenImpl = async (name, cb) => event.listen(name, (e) => cb(e.payload as never));
  } else {
    const mock = await import("./mock");
    invokeImpl = mock.invoke;
    listenImpl = mock.listen;
  }
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  await backend();
  return invokeImpl!<T>(cmd, args);
}

export async function on<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  await backend();
  return listenImpl!<T>(event, cb);
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  openPrivacySettings: () => call<void>("open_privacy_settings"),
  listVolumes: () => call<Volume[]>("list_volumes"),
  startScan: (path: string, record = true) => call<number>("start_scan", { path, record }),
  cancelScan: () => call<void>("cancel_scan"),
  summary: () => call<Summary>("summary"),
  issues: (limit: number) => call<ScanIssue[]>("issues", { limit }),
  children: (id: number) => call<Node[]>("children", { id }),
  detail: (id: number) => call<Detail>("detail", { id }),
  slice: (id: number, maxDepth: number, minBytes: number, maxNodes: number) =>
    call<Slice>("slice", { id, maxDepth, minBytes: Math.floor(minBytes), maxNodes }),
  topFiles: (id: number, limit: number) => call<Located[]>("top_files", { id, limit }),
  extensions: (id: number, limit: number) => call<ExtRow[]>("extensions", { id, limit }),
  search: (id: number, filter: Filter, limit: number) =>
    call<SearchRows>("search", { id, filter, limit }),
  oldAndLarge: (id: number, options: OldLargeOptions) =>
    call<OldLargeRows>("old_and_large", { id, options }),
  emptyFolders: (id: number, limit: number) => call<EmptyRows>("empty_folders", { id, limit }),
  brokenShortcuts: (id: number, limit: number) =>
    call<ShortcutRows>("broken_shortcuts", { id, limit }),
  historySnapshots: (root: string) => call<Snapshot[]>("history_snapshots", { root }),
  historyDiff: (older: number, newer: number, minDelta: number, limit: number) =>
    call<Diff>("history_diff", { older, newer, minDelta, limit }),
  historyInfo: () => call<HistoryInfo>("history_info"),
  historyClear: () => call<void>("history_clear"),
  historyForget: (root: string) => call<number>("history_forget", { root }),
  findPath: (path: string) => call<number | null>("find_path", { path }),
  explain: (id: number, lang: string) => call<Article | null>("explain", { id, lang }),
  ageMatrix: (id: number, limit: number) => call<AgeMatrix>("age_matrix", { id, limit }),
  owner: (id: number) => call<string | null>("owner", { id }),
  takeStartupPath: () => call<string | null>("take_startup_path"),
  shellMenuStatus: () => call<boolean>("shell_menu_status"),
  updateCheck: () => call<UpdateInfo>("update_check"),
  updateInstall: () => call<void>("update_install"),
  updateOpenReleases: () => call<void>("update_open_releases"),
  shellMenuSet: (enabled: boolean, caption: string, dupesCaption: string) =>
    call<boolean>("shell_menu_set", { enabled, caption, dupesCaption }),
  brokenLinks: (id: number, limit: number) => call<ShortcutRows>("broken_links", { id, limit }),
  health: () => call<HealthScore>("health"),
  reveal: (id: number) => call<void>("reveal", { id }),
  openItem: (id: number) => call<void>("open_item", { id }),
  planDelete: (ids: number[]) => call<DeletePlan>("plan_delete", { ids }),
  recycle: (ids: number[]) => call<RecycleOutcome[]>("recycle", { ids }),
  exportReport: (path: string, format: "json" | "csv") =>
    call<void>("export_report", { path, format }),
  saveHtmlReport: (path: string, html: string) => call<void>("save_html_report", { path, html }),
  loadSettings: () => call<unknown>("load_settings"),
  saveSettings: (settings: unknown) => call<void>("save_settings", { settings }),
  readTextFile: (path: string) => call<string>("read_text_file", { path }),
  writeTextFile: (path: string, contents: string) =>
    call<void>("write_text_file", { path, contents }),
  adviceSummary: () => call<AdviceSummary>("advice_summary"),
  adviceDetail: (id: number) => call<EntryAdvice | null>("advice_detail", { id }),
  adviceRefresh: (rulesDir: string | null = null) => call<void>("advice_refresh", { rulesDir }),
  adviceClean: (keys: string[], caution: boolean, expert: boolean) =>
    call<AdviceOutcome>("advice_clean", { keys, caution, expert }),
  adviceCancel: () => call<void>("advice_cancel"),
  adviceOverride: (ids: number[], ack: boolean) => call<AdviceOutcome>("advice_override", { ids, ack }),
};

export type ProgressEvent = Progress;
export type FinishedEvent = Finished;

/** Native folder picker; `null` when cancelled or unavailable. */
export async function pickFolder(title: string): Promise<string | null> {
  if (!inTauri) return window.prompt(title, "C:\\Users");
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false, title });
  return typeof picked === "string" ? picked : null;
}

export async function pickSaveFile(
  title: string,
  defaultPath: string,
  ext: string,
): Promise<string | null> {
  if (!inTauri) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ title, defaultPath, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
}

export async function pickOpenFile(title: string, ext: string): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    title,
    multiple: false,
    filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
  });
  return typeof picked === "string" ? picked : null;
}

/** Normalize thrown command errors into a message. */
export function errorText(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}
