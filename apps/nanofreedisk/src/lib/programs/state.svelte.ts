// Programs view state (docs/UNINSTALLER.md §2.7): the program list, the uninstall
// queue (one vendor uninstaller at a time), the leftover reports waiting for the
// user, and the software junk scan.

import { on } from "../api";
import type { CleanProgress, OperationResult } from "../clean/types";
import { formatSize } from "../format";
import { intlLocale, t } from "../i18n.svelte";
import { app, fail, toast } from "../state.svelte";
import { programsApi } from "./api";
import type { JunkReport, LeftoverReport, Program, UninstallOutcome, UninstallProgress } from "./types";
import type { ProgramFilter, ProgramSort } from "./model";

export type ProgramsTab = "installed" | "junk";

export const pg = $state({
  tab: "installed" as ProgramsTab,
  list: null as Program[] | null,
  loading: false,
  store: true,
  query: "",
  filter: "all" as ProgramFilter,
  sort: "name" as ProgramSort,
  /** Program ids ticked for a batch uninstall. */
  picked: [] as string[],
  /** The uninstall confirmation, for these programs. */
  confirm: null as null | { ids: string[] },
  quiet: false,
  /** The running uninstall queue. */
  queue: [] as string[],
  current: null as null | { id: string; name: string; phase: "starting" | "running" | "waiting" | "scanning"; alive: number },
  /** Programs still installed after their uninstaller ran (the user cancelled it). */
  still: [] as UninstallOutcome[],
  /** Leftover reports waiting for the user, newest first. */
  leftovers: [] as LeftoverReport[],
  /** Ticked leftovers per program id. */
  leftPicked: {} as Record<string, string[]>,
  /** A leftover removal is running for this program. */
  removing: null as string | null,
  junk: null as JunkReport | null,
  junkLoading: false,
  junkPicked: [] as string[],
  junkConfirm: false,
  running: null as CleanProgress | null,
});

let listing: Promise<void> | null = null;

/** List the installed programs (concurrent calls share one listing). */
export function loadPrograms(): Promise<void> {
  if (listing) return listing;
  pg.loading = true;
  listing = programsApi
    .list(pg.store)
    .then((l) => {
      pg.list = l;
      pg.picked = pg.picked.filter((id) => l.some((p) => p.id === id));
    })
    .catch(fail)
    .finally(() => {
      pg.loading = false;
      listing = null;
    });
  return listing;
}

export const programOf = (id: string): Program | undefined => pg.list?.find((p) => p.id === id);

let progressOff: (() => void) | null = null;

/** Keep the leftover report of a program for review, its 🟢 items ticked. */
function keep(report: LeftoverReport): void {
  pg.leftovers = [report, ...pg.leftovers.filter((r) => r.program.id !== report.program.id)];
  pg.leftPicked[report.program.id] = report.items.filter((i) => i.selected).map((i) => i.id);
}

/** Uninstall `ids` one after the other: each vendor uninstaller runs in its own
 *  window, then the leftovers of what was removed are looked for. */
export async function runUninstalls(ids: string[]): Promise<void> {
  if (pg.current || !ids.length) return;
  pg.confirm = null;
  pg.queue = [...ids];
  pg.still = [];
  progressOff ??= await on<UninstallProgress>("uninstall-progress", (p) => {
    if (pg.current?.id === p.id) {
      pg.current.phase = p.phase;
      pg.current.alive = p.alive;
    }
  });
  try {
    while (pg.queue.length) {
      const id = pg.queue.shift()!;
      const p = programOf(id);
      if (!p) continue;
      pg.current = { id, name: p.name, phase: "starting", alive: 0 };
      let outcome: UninstallOutcome;
      try {
        outcome = await programsApi.uninstall(id, pg.quiet);
      } catch (e) {
        fail(e);
        continue;
      }
      if (outcome.status === "declined") {
        toast(t("pg-declined", { name: p.name }), "error");
        continue;
      }
      if (outcome.status === "failed") {
        toast(t("pg-failed", { name: p.name, message: outcome.message ?? "" }), "error");
        continue;
      }
      if (outcome.status === "still-installed") {
        pg.still = [...pg.still, outcome];
        toast(t("pg-still-installed", { name: p.name }), "info");
        continue;
      }
      pg.current.phase = "scanning";
      try {
        keep(await programsApi.leftovers(id, false));
        toast(t("pg-uninstalled", { name: p.name }), "ok");
      } catch (e) {
        fail(e);
      }
    }
  } finally {
    pg.current = null;
    pg.queue = [];
    pg.picked = [];
    app.version++;
    void loadPrograms();
  }
}

/** "It is finished": stop waiting for the running uninstaller. */
export function stopWaiting(): void {
  programsApi.stopWaiting().catch(fail);
}

/** Expert: look for the leftovers of a program that still looks installed. */
export async function forcedLeftovers(id: string): Promise<void> {
  try {
    keep(await programsApi.leftovers(id, true));
    pg.still = pg.still.filter((s) => s.id !== id);
  } catch (e) {
    fail(e);
  }
}

/** Remove the ticked leftovers of program `id`. */
export async function removeLeftovers(id: string): Promise<OperationResult | null> {
  const picked = pg.leftPicked[id] ?? [];
  if (!picked.length || pg.removing) return null;
  pg.removing = id;
  pg.running = { done: 0, total: picked.length, freed: 0, path: "" };
  try {
    cleanOff ??= await on<CleanProgress>("clean-progress", (p) => {
      if (pg.running) pg.running = p;
    });
    const r = await programsApi.removeLeftovers(id, picked);
    report(r, "pg-left-done");
    pg.leftovers = pg.leftovers.filter((x) => x.program.id !== id);
    delete pg.leftPicked[id];
    return r;
  } catch (e) {
    fail(e);
    return null;
  } finally {
    pg.removing = null;
    pg.running = null;
    app.version++;
  }
}

/** Close a leftover report without removing anything. */
export function dismissLeftovers(id: string): void {
  pg.leftovers = pg.leftovers.filter((x) => x.program.id !== id);
  delete pg.leftPicked[id];
}

let cleanOff: (() => void) | null = null;

function report(r: OperationResult, key: string): void {
  const loc = intlLocale();
  const skippedAdmin = r.problems.filter((p) => p.reason === "uac-declined").length;
  toast(t(key, { size: formatSize(r.freed, loc), done: r.done + r.partial }), r.failed ? "error" : "ok");
  if (skippedAdmin) toast(t("pg-admin-declined", { count: skippedAdmin }), "error");
  if (r.journalError) toast(t("clean-journal-failed", { error: r.journalError }), "error");
}

let junkLoading: Promise<void> | null = null;

/** Look for software junk (read-only). */
export function loadJunk(): Promise<void> {
  if (junkLoading) return junkLoading;
  pg.junkLoading = true;
  junkLoading = programsApi
    .junkScan()
    .then((r) => {
      pg.junk = r;
      pg.junkPicked = r.groups.flatMap((g) => g.items.filter((i) => i.selected).map((i) => i.path));
    })
    .catch(fail)
    .finally(() => {
      pg.junkLoading = false;
      junkLoading = null;
    });
  return junkLoading;
}

/** Clean the ticked software junk into the quarantine. */
export async function cleanJunk(): Promise<void> {
  const paths = [...pg.junkPicked];
  pg.junkConfirm = false;
  if (!paths.length || pg.running) return;
  pg.running = { done: 0, total: paths.length, freed: 0, path: "" };
  try {
    cleanOff ??= await on<CleanProgress>("clean-progress", (p) => {
      if (pg.running) pg.running = p;
    });
    report(await programsApi.junkClean(paths), "pg-junk-done");
  } catch (e) {
    fail(e);
  } finally {
    pg.running = null;
    pg.junk = null;
    app.version++;
    void loadJunk();
  }
}
