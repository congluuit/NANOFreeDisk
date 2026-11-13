// Quick clean (docs/QUICK_CLEAN.md): one button scans with every tool, one screen
// shows what was found in three groups, one confirmation runs the ticked parts.
//
// Each step calls the scan of the tool it belongs to, and each part of the clean
// calls that tool's own run function (Analyze suggestions, Tidy up, Junk files, Dev
// caches), so every item still goes Planner, SafetyGate, Executor and lands in the
// quarantine, restorable from Cleanup history for 7 days. This module only decides
// the order and keeps one result screen.

import { api, errorText } from "../api";
import { cleanAdvice, listenAdvice, loadAdvice } from "../advice/state.svelte";
import { cleanApi } from "../clean/api";
import { leftoverGroups } from "../clean/leftovers";
import { clean, runClean, scanJunk } from "../clean/state.svelte";
import type { CleanScan, LeftoverGroup } from "../clean/types";
import type { DevCacheReport } from "../devcache/types";
import { devApi } from "../devcache/api";
import { dev, runDev, scanDev } from "../devcache/state.svelte";
import { dupesApi } from "../dupes/api";
import { dupes, findPersonal } from "../dupes/state.svelte";
import { formatCount, formatSize } from "../format";
import { intlLocale, t } from "../i18n.svelte";
import { currentOs, hasFeature } from "../platform";
import { recycleBinApi } from "../recyclebin/api";
import { afterDelete, app, expert, fail, muteToasts, oldLargeOptions, refreshVolumes, startScan, toast, waitScan } from "../state.svelte";
import type { DeletePlan } from "../types";
import {
  EMPTY_INPUTS,
  buildRows,
  cancelOpen,
  defaultTicks,
  moveStep,
  partOf,
  planSteps,
  runPlan,
  sanitizeTicks,
  skipDependents,
  waitingPart,
  type Inputs,
  type Part,
  type Row,
  type RowId,
  type Step,
  type StepId,
  type Task,
  type Tool,
} from "./model";

/** A scan of the same drive younger than this is used as is. */
const REUSE_SECONDS = 15 * 60;
/** How long the scan's suggestions may take to be ready. */
const ADVICE_WAIT_MS = 120_000;

export type QuickPhase = "idle" | "scanning" | "ready" | "cleaning" | "done";

export const quick = $state({
  phase: "idle" as QuickPhase,
  steps: [] as Step[],
  rows: [] as Row[],
  ticks: [] as RowId[],
  /** The scan was stopped before every step ended. */
  stopped: false,
  stopping: false,
  /** The drive scan is Quick clean's own: its progress shows in the step list. */
  ownsScan: false,
  /** The confirmation is open. */
  confirm: false,
  /** What Tidy up's planner allows of the ticked empty folders and shortcuts. */
  tidyPlan: null as DeletePlan | null,
  /** The parts of the clean, in the order they run. */
  parts: [] as Part[],
  /** What the junk clean left, grouped by why. */
  leftovers: [] as LeftoverGroup[],
  /** Unix seconds when the scan / the clean finished. */
  scannedAt: 0,
  cleanedAt: 0,
});

let data: Inputs = { ...EMPTY_INPUTS };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const now = () => Math.floor(Date.now() / 1000);
const sameRoot = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();

class Stopped extends Error {}

function setStep(id: StepId, status: Step["status"], patch: Partial<Omit<Step, "id" | "status">> = {}): void {
  quick.steps = moveStep(quick.steps, id, status, patch);
}

/** The system drive, once the drive list is known. */
async function systemDrive(): Promise<string | null> {
  if (!app.volumes.length) await refreshVolumes();
  return app.volumes.find((v) => v.system && v.ready)?.root ?? null;
}

// ── The steps ──

type Found = { bytes: number | null; count: number | null; note?: string | null };

async function stepJunk(): Promise<Found> {
  if (clean.running) throw new Error(t("quick-busy"));
  clean.scan = null;
  await scanJunk();
  const s = clean.scan as CleanScan | null;
  if (!s) throw new Error(t("quick-step-failed"));
  data.junk = s;
  return { bytes: s.safe + s.caution, count: null };
}

async function stepDev(): Promise<Found> {
  if (dev.running) throw new Error(t("quick-busy"));
  dev.report = null;
  await scanDev();
  const r = dev.report as DevCacheReport | null;
  if (!r) throw new Error(t("quick-step-failed"));
  data.dev = r;
  return { bytes: r.safe, count: null };
}

async function stepBin(): Promise<Found> {
  const b = await recycleBinApi.status();
  data.bin = b;
  return { bytes: b.bytes, count: b.items };
}

async function stepScan(): Promise<Found> {
  const root = await systemDrive();
  if (!root) throw new Error(t("quick-note-no-drive"));
  data.root = root;
  const s = app.summary;
  if (!app.scanning && s && sameRoot(s.root, root) && now() - s.scannedAt < REUSE_SECONDS) {
    return { bytes: s.stats.on_disk, count: s.stats.files, note: "quick-note-reused" };
  }
  // someone else's scan is running: let it end first
  if (app.scanning) await waitScan();
  if (quick.stopping) throw new Stopped();
  const end = waitScan();
  quick.ownsScan = true;
  try {
    await startScan(root, "stay");
    const f = await end;
    if (f.cancelled) throw new Stopped();
    if (!f.ok || !app.summary) throw new Error(f.error ?? t("quick-step-failed"));
  } finally {
    quick.ownsScan = false;
  }
  return { bytes: app.summary.stats.on_disk, count: app.summary.stats.files };
}

async function stepTidy(): Promise<Found> {
  const id = app.summary?.rootId ?? 0;
  const [empty, links] = await Promise.all([api.emptyFolders(id, 2000), api.brokenShortcuts(id, 2000)]);
  data.empty = empty;
  data.links = links;
  return { bytes: null, count: empty.rows.length + links.rows.filter((r) => r.status === "missing").length };
}

async function stepAdvice(): Promise<Found> {
  await listenAdvice();
  const until = Date.now() + ADVICE_WAIT_MS;
  while (Date.now() < until) {
    if (quick.stopping) throw new Stopped();
    const s = await api.adviceSummary();
    if (!s.computing && s.suggestions) {
      data.advice = s;
      void loadAdvice();
      return { bytes: null, count: null };
    }
    await sleep(500);
  }
  throw new Error(t("quick-advice-timeout"));
}

async function stepLarge(): Promise<Found> {
  const r = await api.oldAndLarge(app.summary?.rootId ?? 0, oldLargeOptions());
  data.large = r;
  return { bytes: r.distinctOnDisk, count: r.distinctFiles };
}

async function stepDupes(): Promise<Found> {
  // a search the user started in Duplicates: wait for it instead of starting another
  while (dupes.searching) {
    if (quick.stopping) throw new Stopped();
    await sleep(300);
  }
  await findPersonal();
  if (quick.stopping) throw new Stopped();
  const r = dupes.report;
  if (!r) throw new Error(t("quick-step-failed"));
  data.dupes = r;
  return { bytes: r.wastedBytes, count: r.totalGroups || r.groups.length };
}

const RUNNERS: Record<StepId, () => Promise<Found>> = {
  junk: stepJunk,
  dev: stepDev,
  bin: stepBin,
  scan: stepScan,
  tidy: stepTidy,
  advice: stepAdvice,
  large: stepLarge,
  dupes: stepDupes,
};

let current: StepId | null = null;

/** Scan with every tool, one step after the other. A failed step does not stop the
 * others; what reads the drive scan is skipped when it did not finish. */
export async function scanAll(): Promise<void> {
  if (quick.phase === "scanning" || quick.phase === "cleaning") return;
  const os = currentOs(app.info?.os);
  const drive = await systemDrive();
  data = { ...EMPTY_INPUTS };
  quick.steps = planSteps({ junk: hasFeature(os, "clean"), bin: hasFeature(os, "recycle-bin"), drive: !!drive });
  quick.rows = [];
  quick.ticks = [];
  quick.parts = [];
  quick.leftovers = [];
  quick.stopped = false;
  quick.stopping = false;
  quick.phase = "scanning";
  for (const step of quick.steps.slice()) {
    if (quick.stopping) break;
    if (quick.steps.find((s) => s.id === step.id)?.status !== "waiting") continue;
    current = step.id;
    setStep(step.id, "running");
    try {
      const found = await RUNNERS[step.id]();
      setStep(step.id, "done", { bytes: found.bytes, count: found.count, note: found.note ?? null });
    } catch (e) {
      if (e instanceof Stopped || quick.stopping) setStep(step.id, "cancelled");
      else setStep(step.id, "failed", { error: e instanceof Error ? e.message : errorText(e) });
      if (step.id === "scan") quick.steps = skipDependents(quick.steps);
    }
  }
  current = null;
  if (quick.stopping) {
    quick.steps = cancelOpen(quick.steps);
    quick.stopped = true;
  }
  quick.stopping = false;
  refreshRows();
  quick.scannedAt = now();
  quick.phase = "ready";
}

/** Rebuild the result rows from what the steps found (also after the Recycle Bin
 * was emptied); the user's ticks stay where they still apply. */
function refreshRows(keepTicks = false): void {
  const rows = buildRows(data);
  // what the drive scan added beyond the junk and caches above
  const advice = rows.find((r) => r.id === "advice");
  if (data.advice) quick.steps = quick.steps.map((s) => (s.id === "advice" && s.status === "done" ? { ...s, bytes: advice?.bytes ?? 0, count: advice?.count ?? 0 } : s));
  const devRow = rows.find((r) => r.id === "dev");
  if (data.dev) quick.steps = quick.steps.map((s) => (s.id === "dev" && s.status === "done" ? { ...s, bytes: devRow?.bytes ?? 0 } : s));
  quick.rows = rows;
  quick.ticks = keepTicks ? sanitizeTicks(rows, quick.ticks) : defaultTicks(rows);
}

/** Stop the scan: the running step is asked to stop, the rest is not started. */
export async function stopScan(): Promise<void> {
  if (quick.phase !== "scanning" || quick.stopping) return;
  quick.stopping = true;
  try {
    if (current === "scan" && app.scanning) await api.cancelScan();
    else if (current === "dupes") await dupesApi.cancel();
  } catch (e) {
    fail(e);
  }
}

export function toggleTick(id: RowId): void {
  const row = quick.rows.find((r) => r.id === id);
  if (!row?.tickable || quick.phase !== "ready") return;
  quick.ticks = quick.ticks.includes(id) ? quick.ticks.filter((x) => x !== id) : [...quick.ticks, id];
}

// ── Confirmation and the clean ──

/** Open the one confirmation. Tidy up's planner is asked first, like its own dialog
 * does, so the confirmation lists what it allows and what stays. */
export async function askClean(): Promise<void> {
  const plan = runPlan(quick.rows, quick.ticks);
  if (!plan.length || quick.phase !== "ready") return;
  quick.tidyPlan = null;
  const tidy = plan.find((p) => p.tool === "tidy");
  if (tidy) {
    try {
      quick.tidyPlan = await api.planDelete(tidy.ids);
    } catch (e) {
      fail(e);
      return;
    }
  }
  quick.confirm = true;
}

let stopRun = false;
let running: Tool | null = null;

function setPart(tool: Tool, part: Partial<Part>): void {
  quick.parts = quick.parts.map((p) => (p.tool === tool ? { ...p, ...part } : p));
}

async function runAdvice(task: Task): Promise<Part> {
  const out = await cleanAdvice({ keys: task.keys, rows: [], items: [], touched: task.ids }, false);
  if (!out) return partOf("advice", null, t("quick-part-not-run"));
  const op = out.operation;
  const recycled = out.recycled.filter((r) => r.status === "done");
  const notDone = out.recycled.filter((r) => r.status !== "done");
  const reasons = new Map<string, number>();
  for (const r of notDone) reasons.set(r.reason ?? r.status, (reasons.get(r.reason ?? r.status) ?? 0) + 1);
  for (const [, code] of out.skipped) reasons.set(code, (reasons.get(code) ?? 0) + 1);
  return {
    tool: "advice",
    status: op?.cancelled ? "cancelled" : "done",
    freed: (op?.freed ?? 0) + recycled.reduce((s, r) => s + r.onDisk, 0),
    done: (op?.done ?? 0) + (op?.partial ?? 0) + recycled.length,
    stayed: (op?.skipped ?? 0) + (op?.failed ?? 0) + notDone.length + out.skipped.length,
    reasons: [...reasons.entries()].map(([code, count]) => ({ code, count })),
    error: null,
  };
}

/** Tidy up's own path (the delete dialog's): only what the planner allowed goes, to
 * the quarantine, through `delete_entries` (the SafetyGate checks every item again). */
async function runTidy(): Promise<Part> {
  const plan = quick.tidyPlan;
  const allowed = plan?.items.filter((i) => i.allowed) ?? [];
  const blocked = plan?.items.filter((i) => !i.allowed) ?? [];
  const reasons = new Map<string, number>();
  for (const b of blocked) reasons.set(b.reason ?? "unresolvable", (reasons.get(b.reason ?? "unresolvable") ?? 0) + 1);
  if (!allowed.length) return { ...waitingPart("tidy"), status: "done", stayed: blocked.length, reasons: [...reasons.entries()].map(([code, count]) => ({ code, count })) };
  const out = await cleanApi.deleteEntries(allowed.map((i) => i.id), "quarantine", "tidy");
  const done = out.filter((o) => o.status === "done");
  for (const o of out) {
    if (o.status === "done") continue;
    const code = o.reason ?? o.status;
    reasons.set(code, (reasons.get(code) ?? 0) + 1);
    if (o.status === "failed" && o.message) toast(o.message, "error");
  }
  await afterDelete(done.map((o) => o.id));
  return {
    tool: "tidy",
    status: "done",
    freed: done.reduce((s, o) => s + o.onDisk, 0),
    done: done.length,
    stayed: out.length - done.length + blocked.length,
    reasons: [...reasons.entries()].map(([code, count]) => ({ code, count })),
    error: null,
  };
}

async function runJunk(task: Task): Promise<Part> {
  if (clean.running) throw new Error(t("quick-busy"));
  await runClean(task.ruleIds, expert(), false);
  quick.leftovers = leftoverGroups(clean.last);
  return partOf("junk", clean.last, clean.last ? null : t("quick-part-not-run"));
}

async function runDevCaches(task: Task): Promise<Part> {
  if (dev.running) throw new Error(t("quick-busy"));
  await runDev(task.ruleIds, expert(), false);
  return partOf("dev", dev.last, dev.last ? null : t("quick-part-not-run"));
}

/** Run the ticked parts (already confirmed), one tool after the other. */
export async function runQuick(): Promise<void> {
  const plan = runPlan(quick.rows, quick.ticks);
  quick.confirm = false;
  if (!plan.length || quick.phase !== "ready") return;
  quick.phase = "cleaning";
  quick.parts = plan.map((p) => waitingPart(p.tool));
  quick.leftovers = [];
  stopRun = false;
  muteToasts(true);
  try {
    for (const task of plan) {
      if (stopRun) {
        setPart(task.tool, { status: "cancelled" });
        continue;
      }
      running = task.tool;
      setPart(task.tool, { status: "running" });
      try {
        const part =
          task.tool === "advice" ? await runAdvice(task) : task.tool === "tidy" ? await runTidy() : task.tool === "junk" ? await runJunk(task) : await runDevCaches(task);
        setPart(task.tool, part);
      } catch (e) {
        setPart(task.tool, { status: "failed", error: errorText(e) });
      }
    }
  } finally {
    running = null;
    muteToasts(false);
  }
  quick.tidyPlan = null;
  quick.cleanedAt = now();
  quick.phase = "done";
  const freed = quick.parts.reduce((s, p) => s + p.freed, 0);
  app.settings.lastQuick = { at: quick.cleanedAt, freed };
  const loc = intlLocale();
  toast(t("quick-done-toast", { size: formatSize(freed, loc), count: formatCount(quick.parts.reduce((s, p) => s + p.done, 0), loc) }), "ok");
}

/** Stop the clean: the running tool is asked to stop, the rest is not started. */
export async function stopQuick(): Promise<void> {
  if (quick.phase !== "cleaning") return;
  stopRun = true;
  try {
    if (running === "junk") await cleanApi.cancel();
    else if (running === "dev") await devApi.cancel();
    else if (running === "advice") await api.adviceCancel();
  } catch (e) {
    fail(e);
  }
}

/** Back to the result after a clean, or to the start. */
export function resetQuick(): void {
  if (quick.phase === "scanning" || quick.phase === "cleaning") return;
  quick.phase = "idle";
  quick.steps = [];
  quick.rows = [];
  quick.ticks = [];
  quick.parts = [];
  quick.leftovers = [];
}

/** Open Quick clean; with nothing scanned yet, start the scan right away. */
export function openQuick(start = true): void {
  app.view = "quick";
  if (start && quick.phase === "idle") void scanAll();
}

// ── The Recycle Bin (its own confirmation; emptying is permanent) ──

export async function emptyBin(): Promise<void> {
  try {
    const r = await recycleBinApi.empty();
    const loc = intlLocale();
    toast(t("rb-emptied", { count: r.items, size: formatSize(r.bytes, loc) }), r.failed.length ? "error" : "ok");
    for (const f of r.failed.slice(0, 3)) toast(t("rb-empty-failed", { error: f }), "error");
    if (r.journalError) toast(t("clean-journal-failed", { error: r.journalError }), "error");
    data.bin = await recycleBinApi.status();
    refreshRows(true);
  } catch (e) {
    fail(e);
  } finally {
    app.version++;
  }
}

/** Open the Windows tool of a guidance rule. */
export async function openTool(ruleId: string): Promise<void> {
  try {
    await cleanApi.openTool(ruleId);
  } catch (e) {
    fail(e);
  }
}
