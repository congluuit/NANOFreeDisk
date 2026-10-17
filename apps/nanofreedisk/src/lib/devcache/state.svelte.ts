// DevCache view state: the last scans of each tab, the running clean / command and
// the threshold reminders (PHASE4_DEVCACHE §2.5).

import { on } from "../api";
import type { CleanProgress, OperationResult } from "../clean/types";
import { formatSize } from "../format";
import { intlLocale, t } from "../i18n.svelte";
import { app, fail, toast } from "../state.svelte";
import { aiPreselected } from "./ai";
import { devApi } from "./api";
import type { AiPlan, AiReport, DevCacheReport, DevRule, DevTab, RepoScan, StoreInfo, StoreRun, Toolchain, UnknownFolder } from "./types";

export const dev = $state({
  tab: "cache" as DevTab,
  report: null as DevCacheReport | null,
  scanning: false,
  /** Rules ticked on the Cache tab. */
  picked: [] as string[],
  /** Confirmation dialog open for these rule ids. */
  confirm: null as null | string[],
  running: null as CleanProgress | null,
  last: null as OperationResult | null,
  unknown: null as UnknownFolder[] | null,
  unknownLoading: false,
  tools: null as Toolchain[] | null,
  toolsLoading: false,
  repos: null as RepoScan | null,
  reposLoading: false,
  /** Artifact paths ticked on the Repo tab. */
  repoPicked: [] as string[],
  repoConfirm: false,
  stores: null as StoreInfo[] | null,
  storesLoading: false,
  storeConfirm: null as StoreInfo | null,
  storeRunning: null as string | null,
  storeLast: null as StoreRun | null,
  ai: null as AiReport | null,
  aiLoading: false,
  /** Item / build-output paths ticked on the AI leftovers tab. */
  aiPicked: [] as string[],
  /** Confirmation dialog: what cleaning the ticked paths runs. */
  aiConfirm: null as AiPlan | null,
});

/** Every rule of the report, in vendor order. */
export function allRules(report: DevCacheReport | null): DevRule[] {
  return report?.vendors.flatMap((v) => v.apps.flatMap((a) => a.rules)) ?? [];
}

/** 🟢 Safe rules with something to clean, optionally of one vendor. */
export function safeIds(report: DevCacheReport | null, vendor?: string): string[] {
  return (report?.vendors ?? [])
    .filter((v) => !vendor || v.vendor === vendor)
    .flatMap((v) => v.apps.flatMap((a) => a.rules))
    .filter((r) => r.level === "safe" && r.candidates > 0)
    .map((r) => r.ruleId);
}

export function bytesOf(ids: string[]): number {
  return allRules(dev.report)
    .filter((r) => ids.includes(r.ruleId))
    .reduce((s, r) => s + r.bytes, 0);
}

/** Vendor thresholds in GB for the backend (0 = no reminder). */
function thresholds(): Record<string, number> {
  return { ...app.settings.devThresholds };
}

const testRulesDir = (): string | undefined => (globalThis as { __nfdDevRulesDir?: string }).__nfdDevRulesDir;

let scanning: Promise<void> | null = null;

/** Match the DevCache packs (read-only). Concurrent calls share one scan. */
export function scanDev(remind = false): Promise<void> {
  if (scanning) return scanning;
  dev.scanning = true;
  scanning = devApi
    .scan(thresholds(), testRulesDir())
    .then((r) => {
      dev.report = r;
      dev.picked = safeIds(r);
      if (remind) void remindOver(r);
    })
    .catch(fail)
    .finally(() => {
      dev.scanning = false;
      scanning = null;
    });
  return scanning;
}

let reminded = "";

/** Notify (system + toast) about vendors over their threshold, once per set. */
async function remindOver(r: DevCacheReport): Promise<void> {
  if (!app.settings.devAlerts) return;
  const over = r.vendors.filter((v) => v.overThreshold);
  const sig = over.map((v) => v.vendor).join("|");
  if (!over.length || sig === reminded) return;
  reminded = sig;
  const loc = intlLocale();
  const list = over.map((v) => `${v.vendor} ${formatSize(v.bytes, loc)}`).join(", ");
  toast(t("dev-over-toast", { list }), "info");
  await devApi.notify(t("dev-over-title"), t("dev-over-body", { list })).catch(() => {});
}

/** Reminder check a while after the app starts (only when reminders are on). */
export function scheduleReminder(): void {
  setTimeout(() => {
    if (app.settings.devAlerts && !dev.report) void scanDev(true);
  }, 60_000);
}

let progressOff: (() => void) | null = null;

async function listenProgress(): Promise<void> {
  progressOff ??= await on<CleanProgress>("clean-progress", (p) => {
    if (dev.running) dev.running = p;
  });
}

function reportResult(result: OperationResult): void {
  dev.last = result;
  toast(
    t("clean-result", { size: formatSize(result.freed, intlLocale()), done: result.done + result.partial, skipped: result.skipped + result.failed }),
    result.failed ? "error" : "ok",
  );
  if (result.journalError) toast(t("clean-journal-failed", { error: result.journalError }), "error");
}

/** Clean the ticked rules (already confirmed). */
export async function runDev(ruleIds: string[], expert: boolean, permanent: boolean): Promise<void> {
  if (dev.running) return;
  dev.running = { done: 0, total: 0, freed: 0, path: "" };
  dev.last = null;
  try {
    await listenProgress();
    reportResult(await devApi.run(ruleIds, expert, permanent));
  } catch (e) {
    fail(e);
  } finally {
    dev.running = null;
    dev.report = null;
    app.version++;
    void scanDev();
  }
}

export async function loadUnknown(): Promise<void> {
  if (dev.unknownLoading) return;
  dev.unknownLoading = true;
  try {
    dev.unknown = await devApi.unknown();
  } catch (e) {
    fail(e);
  } finally {
    dev.unknownLoading = false;
  }
}

export async function loadTools(): Promise<void> {
  if (dev.toolsLoading) return;
  dev.toolsLoading = true;
  try {
    dev.tools = await devApi.toolchains();
  } catch (e) {
    fail(e);
  } finally {
    dev.toolsLoading = false;
  }
}

export async function loadRepos(root: string): Promise<void> {
  if (dev.reposLoading || !root) return;
  dev.reposLoading = true;
  try {
    const s = app.settings;
    const scan = await devApi.repos(root, s.keepRecentDays, s.staleDays, true);
    dev.repos = scan;
    dev.repoPicked = scan.repos.flatMap((r) => r.artifacts.filter((a) => a.selected).map((a) => a.path));
  } catch (e) {
    fail(e);
  } finally {
    dev.reposLoading = false;
  }
}

/** Move the ticked build output to the quarantine (already confirmed). */
export async function cleanRepos(paths: string[]): Promise<void> {
  if (dev.running || !paths.length) return;
  const root = dev.repos?.root ?? "";
  dev.running = { done: 0, total: 0, freed: 0, path: "" };
  dev.last = null;
  try {
    await listenProgress();
    reportResult(await devApi.reposClean(paths));
  } catch (e) {
    fail(e);
  } finally {
    dev.running = null;
    dev.repos = null;
    app.version++;
    void loadRepos(root);
  }
}

export async function loadStores(): Promise<void> {
  if (dev.storesLoading) return;
  dev.storesLoading = true;
  try {
    dev.stores = await devApi.stores();
  } catch (e) {
    fail(e);
  } finally {
    dev.storesLoading = false;
  }
}

/** Run a confirmed package-store command: `commandLine` is what the dialog showed. */
export async function runStore(info: StoreInfo): Promise<void> {
  if (dev.storeRunning) return;
  dev.storeRunning = info.id;
  dev.storeLast = null;
  try {
    const r = await devApi.storeRun(info.id, info.commandLine);
    dev.storeLast = r;
    const ok = r.output.code === 0 && !r.output.timedOut && !r.output.cancelled;
    toast(ok ? t("dev-store-done", { size: formatSize(r.freed, intlLocale()) }) : t("dev-store-failed"), ok ? "ok" : "error");
    if (r.journalError) toast(t("clean-journal-failed", { error: r.journalError }), "error");
  } catch (e) {
    fail(e);
  } finally {
    dev.storeRunning = null;
    void loadStores();
  }
}

export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast(t("copied"), "ok");
  } catch (e) {
    fail(e);
  }
}

// ── AI leftovers ──

const aiFixture = (): string | undefined => (globalThis as { __nfdAiFixture?: string }).__nfdAiFixture;

let aiScanning: Promise<void> | null = null;

/** Scan for AI leftovers (read-only). Concurrent calls share one scan; `quiet`
 *  (the background check behind Home and the Health Score) shows no errors. */
export function loadAi(quiet = false): Promise<void> {
  if (aiScanning) return aiScanning;
  dev.aiLoading = true;
  const s = app.settings;
  aiScanning = devApi
    .aiScan(s.repoRoot ? [s.repoRoot] : [], s.keepRecentDays, aiFixture())
    .then((r) => {
      dev.ai = r;
      dev.aiPicked = aiPreselected(r);
    })
    .catch((e) => {
      if (!quiet) fail(e);
    })
    .finally(() => {
      dev.aiLoading = false;
      aiScanning = null;
    });
  return aiScanning;
}

/** Open the confirmation for `paths` (what git will run is shown there). */
export async function confirmAi(paths: string[]): Promise<void> {
  if (!paths.length) return;
  try {
    dev.aiConfirm = await devApi.aiPlan(paths);
    dev.aiPicked = paths;
  } catch (e) {
    fail(e);
  }
}

/** Clean the ticked leftovers (already confirmed). */
export async function cleanAi(paths: string[]): Promise<void> {
  if (dev.running || !paths.length) return;
  dev.running = { done: 0, total: 0, freed: 0, path: "" };
  dev.last = null;
  dev.aiConfirm = null;
  try {
    await listenProgress();
    reportResult(await devApi.aiClean(paths));
  } catch (e) {
    fail(e);
  } finally {
    dev.running = null;
    dev.ai = null;
    app.version++;
    void loadAi();
  }
}
