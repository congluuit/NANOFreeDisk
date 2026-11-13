// Quick clean: the pure part (tested in quick.test.ts). The step list and its state
// machine, the result rows built from what each tool found (with the overlaps taken
// out, so no byte is counted twice), the totals of what is ticked and the order the
// tools run in. Nothing here deletes anything: every row is carried out by the tool
// it comes from, through Planner, SafetyGate and Executor.

import type { CleanScan, OperationResult, RuleSummary } from "../clean/types";
import type { DevCacheReport, DevRule } from "../devcache/types";
import type { DupeReport } from "../dupes/types";
import type { BinStatus } from "../recyclebin/types";
import type { AdviceGroup, AdviceSummary, EmptyRows, OldLargeRows, ShortcutRows } from "../types";

// ── Steps ──

export type StepId = "junk" | "dev" | "bin" | "scan" | "tidy" | "advice" | "large" | "dupes";

/** The order the scan runs in. */
export const STEP_ORDER: StepId[] = ["junk", "dev", "bin", "scan", "tidy", "advice", "large", "dupes"];

/** Steps that read the drive scan. */
export const NEEDS_SCAN: StepId[] = ["tidy", "advice", "large"];

export type StepStatus = "waiting" | "running" | "done" | "skipped" | "failed" | "cancelled";

export interface Step {
  id: StepId;
  status: StepStatus;
  /** What it found (bytes / items); null until done. */
  bytes: number | null;
  count: number | null;
  /** Locale key of a note (why it was skipped, "used the last scan"…). */
  note: string | null;
  error: string | null;
}

/** What this platform and machine offer. */
export interface Platform {
  /** The Windows rule packs (Junk files). */
  junk: boolean;
  /** A Recycle Bin the app can read. */
  bin: boolean;
  /** A system drive to scan. */
  drive: boolean;
}

/** The steps of a scan: what the platform lacks is left out; without a system drive
 * the scan and what reads it are skipped. */
export function planSteps(p: Platform): Step[] {
  return STEP_ORDER.filter((id) => (id === "junk" ? p.junk : id === "bin" ? p.bin : true)).map((id) => {
    const skip = !p.drive && (id === "scan" || NEEDS_SCAN.includes(id));
    return { id, status: skip ? "skipped" : "waiting", bytes: null, count: null, note: skip ? "quick-note-no-drive" : null, error: null };
  });
}

const NEXT: Record<StepStatus, StepStatus[]> = {
  waiting: ["running", "skipped", "cancelled"],
  running: ["done", "failed", "skipped", "cancelled"],
  done: [],
  skipped: [],
  failed: [],
  cancelled: [],
};

/** `true` when a step may go from `from` to `to`. */
export const canMove = (from: StepStatus, to: StepStatus): boolean => NEXT[from].includes(to);

/** Move step `id` to `status` (with what it found); an invalid move changes nothing. */
export function moveStep(steps: Step[], id: StepId, status: StepStatus, patch: Partial<Omit<Step, "id" | "status">> = {}): Step[] {
  const at = steps.findIndex((s) => s.id === id);
  if (at < 0 || !canMove(steps[at].status, status)) return steps;
  const out = steps.slice();
  out[at] = { ...steps[at], ...patch, status };
  return out;
}

/** The scan did not finish: what reads it is skipped. */
export function skipDependents(steps: Step[]): Step[] {
  return NEEDS_SCAN.reduce((acc, id) => moveStep(acc, id, "skipped", { note: "quick-note-needs-scan" }), steps);
}

/** Stop: every step not finished yet is cancelled. */
export function cancelOpen(steps: Step[]): Step[] {
  return steps.map((s) => (s.status === "waiting" || s.status === "running" ? { ...s, status: "cancelled" } : s));
}

export type ScanPhase = "idle" | "running" | "finished";

export function scanPhase(steps: Step[]): ScanPhase {
  if (!steps.length || steps.every((s) => s.status === "waiting")) return "idle";
  return steps.some((s) => s.status === "waiting" || s.status === "running") ? "running" : "finished";
}

/** Share of the steps that ended (for the progress bar). */
export function progressOf(steps: Step[]): number {
  if (!steps.length) return 0;
  return steps.filter((s) => s.status !== "waiting" && s.status !== "running").length / steps.length;
}

// ── Result rows ──

export type Group = "safe" | "look" | "info";

export type RowId =
  | "junk"
  | "dev"
  | "advice"
  | "empty"
  | "links"
  | "junk-caution"
  | "dupes"
  | "large"
  | "more"
  | "bin"
  | "guide"
  | "admin";

/** One line under a row ("Chrome cache 1.2 GB"); an advice group's title is
 * translated by the view. */
export interface Detail {
  label: string;
  group?: AdviceGroup;
  bytes: number;
  /** A guidance rule's Windows tool can be opened for it. */
  ruleId?: string;
}

export interface Row {
  id: RowId;
  group: Group;
  /** What it holds (for a tickable row: what goes when it is ticked). */
  bytes: number;
  /** Of `bytes`, what is likely to stay because an app holding it is open. */
  skipBytes: number;
  count: number;
  /** Can be ticked for the one Clean button. */
  tickable: boolean;
  /** Ticked when the result opens. */
  ticked: boolean;
  /** Needs the acknowledgement in the confirmation (Caution items). */
  caution: boolean;
  /** Apps to close first (their files are skipped while they run). */
  running: string[];
  details: Detail[];
  /** What the tool runs on: rule ids, advice group keys or scan entry ids. */
  ruleIds: string[];
  keys: string[];
  ids: number[];
}

export interface Inputs {
  junk: CleanScan | null;
  dev: DevCacheReport | null;
  bin: BinStatus | null;
  /** The scanned folder (the system drive). */
  root: string | null;
  empty: EmptyRows | null;
  links: ShortcutRows | null;
  advice: AdviceSummary | null;
  large: OldLargeRows | null;
  dupes: DupeReport | null;
}

export const EMPTY_INPUTS: Inputs = { junk: null, dev: null, bin: null, root: null, empty: null, links: null, advice: null, large: null, dupes: null };

const sum = <T>(xs: T[], f: (x: T) => number): number => xs.reduce((s, x) => s + f(x), 0);
const uniq = (xs: string[]): string[] => [...new Set(xs)].sort();

/** A path compared without case and with one kind of separator. */
export function normPath(p: string): string {
  return p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

/** `rel` (relative to `root`, as the advice gives it) as an absolute path. */
export function joinRoot(root: string, rel: string): string {
  if (/^[a-zA-Z]:[\\/]/.test(rel) || rel.startsWith("/") || rel.startsWith("\\\\")) return rel;
  const sep = root.includes("\\") ? "\\" : "/";
  return root.replace(/[\\/]+$/, "") + sep + rel.replace(/^[\\/]+/, "");
}

/** `true` when `path` is one of `prefixes` or lies inside one. */
export function insideAny(path: string, prefixes: string[]): boolean {
  const p = normPath(path);
  return prefixes.some((x) => {
    const q = normPath(x);
    return q !== "" && (p === q || p.startsWith(q + "/"));
  });
}

/** Advice actions Quick clean carries out: both go to the quarantine (restorable for
 * 7 days). AI agent folders (git) and Recycle Bin moves stay in their own views. */
const QUICK_ACTIONS = ["clean", "quarantine"];

const junkDetail = (r: RuleSummary): Detail => ({ label: r.app, bytes: r.bytes });
const runningOf = (rules: RuleSummary[]): string[] => uniq(rules.flatMap((r) => r.running));
const skipOf = (rules: RuleSummary[]): number => sum(rules.filter((r) => r.running.length > 0), (r) => r.bytes);

function ruleRow(id: RowId, group: Group, rules: RuleSummary[], ticked: boolean, caution: boolean): Row | null {
  if (!rules.length) return null;
  const sorted = [...rules].sort((a, b) => b.bytes - a.bytes);
  return {
    id,
    group,
    bytes: sum(rules, (r) => r.bytes),
    skipBytes: skipOf(rules),
    count: sum(rules, (r) => r.files),
    tickable: true,
    ticked,
    caution,
    running: runningOf(rules),
    details: sorted.map(junkDetail),
    ruleIds: sorted.map((r) => r.ruleId),
    keys: [],
    ids: [],
  };
}

function infoRow(id: RowId, group: Group, bytes: number, count: number, details: Detail[] = []): Row {
  return { id, group, bytes, skipBytes: 0, count, tickable: false, ticked: false, caution: false, running: [], details, ruleIds: [], keys: [], ids: [] };
}

const titleDetail = (g: AdviceGroup): Detail => ({ label: g.location ?? g.app ?? g.source ?? "", group: g, bytes: g.bytes });

/** Rule ids the junk scan and the dev-cache scan already cover (any level). */
export function coveredRules(i: Pick<Inputs, "junk" | "dev">): Set<string> {
  const devRules: DevRule[] = i.dev?.vendors.flatMap((v) => v.apps.flatMap((a) => [...a.rules, ...a.kept])) ?? [];
  return new Set([...(i.junk?.rules ?? []).map((r) => r.ruleId), ...devRules.map((r) => r.ruleId)]);
}

/** Advice groups Quick clean ticks: Safe, actionable, pre-selected by the backend, a
 * quarantine action, and not junk a rule scan above already counts. */
export function quickAdvice(i: Pick<Inputs, "junk" | "dev" | "advice">): AdviceGroup[] {
  const covered = coveredRules(i);
  const groups = [...(i.advice?.suggestions?.groups ?? []), ...(i.advice?.elsewhere ?? [])];
  return groups.filter(
    (g) =>
      g.verdict === "safe" &&
      g.actionable &&
      g.selected &&
      !g.blocked &&
      QUICK_ACTIONS.includes(g.action) &&
      !((g.reason === "rule" || g.reason === "keep-rule" || g.elsewhere) && g.source && covered.has(g.source)),
  );
}

/** Advice groups worth a look in the disk map: Caution, or Safe but outside what
 * Quick clean runs (AI agent folders, Recycle Bin moves), not junk counted above. */
export function lookAdvice(i: Pick<Inputs, "junk" | "dev" | "advice">): AdviceGroup[] {
  const covered = coveredRules(i);
  const ticked = new Set(quickAdvice(i).map((g) => g.key));
  return (i.advice?.suggestions?.groups ?? []).filter(
    (g) =>
      (g.verdict === "safe" || g.verdict === "caution") &&
      g.actionable &&
      !ticked.has(g.key) &&
      g.reason !== "old-large" &&
      g.reason !== "old-installer" &&
      g.reason !== "duplicate" &&
      !(g.reason === "rule" && g.source && covered.has(g.source)),
  );
}

/** Every row of the result, in display order (empty rows left out). */
export function buildRows(i: Inputs): Row[] {
  const rows: (Row | null)[] = [];
  const junkRules = (i.junk?.rules ?? []).filter((r) => r.candidates > 0);
  const junkIds = new Set((i.junk?.rules ?? []).map((r) => r.ruleId));

  // Safe to remove
  rows.push(ruleRow("junk", "safe", junkRules.filter((r) => r.level === "safe"), true, false));
  const devRules = (i.dev?.vendors ?? [])
    .flatMap((v) => v.apps.flatMap((a) => a.rules))
    .filter((r) => r.level === "safe" && r.candidates > 0 && !junkIds.has(r.ruleId));
  rows.push(ruleRow("dev", "safe", devRules, true, false));

  const adv = quickAdvice(i);
  const advRoots = i.root ? adv.flatMap((g) => g.items.map((it) => joinRoot(i.root as string, it.path))) : [];
  if (adv.length) {
    rows.push({
      id: "advice",
      group: "safe",
      bytes: sum(adv, (g) => g.bytes),
      skipBytes: 0,
      count: sum(adv, (g) => g.count),
      tickable: true,
      ticked: true,
      caution: false,
      running: uniq(adv.flatMap((g) => g.requiresClosed)),
      details: adv.map(titleDetail).sort((a, b) => b.bytes - a.bytes),
      ruleIds: [],
      keys: adv.map((g) => g.key),
      ids: adv.flatMap((g) => g.items.map((it) => it.id)).filter((x): x is number => x != null),
    });
  }

  // empty folders and broken shortcuts already inside a ticked suggestion go with it
  const empty = (i.empty?.rows ?? []).filter((r) => !insideAny(r.path, advRoots));
  if (empty.length) {
    rows.push({
      ...infoRow("empty", "safe", sum(empty, (r) => r.onDisk), empty.length, empty.slice(0, 50).map((r) => ({ label: r.path, bytes: r.onDisk }))),
      tickable: true,
      ticked: true,
      ids: empty.map((r) => r.id),
    });
  }
  // only shortcuts whose target is gone from a drive that is there (a missing drive
  // may be a USB stick that is just unplugged)
  const links = (i.links?.rows ?? []).filter((r) => r.status === "missing" && !insideAny(r.path, advRoots));
  if (links.length) {
    rows.push({
      ...infoRow("links", "safe", sum(links, (r) => r.onDisk), links.length, links.slice(0, 50).map((r) => ({ label: r.path, bytes: r.onDisk }))),
      tickable: true,
      ticked: true,
      ids: links.map((r) => r.id),
    });
  }

  // Worth a look
  rows.push(ruleRow("junk-caution", "look", junkRules.filter((r) => r.level === "caution"), false, true));
  if (i.dupes && i.dupes.groups.length) {
    rows.push(infoRow("dupes", "look", i.dupes.wastedBytes, i.dupes.totalGroups || i.dupes.groups.length));
  }
  if (i.large && i.large.distinctFiles > 0) rows.push(infoRow("large", "look", i.large.distinctOnDisk, i.large.distinctFiles));
  const more = lookAdvice(i);
  if (more.length) rows.push(infoRow("more", "look", sum(more, (g) => g.bytes), more.length, more.map(titleDetail)));
  if (i.bin && i.bin.items > 0) {
    rows.push(infoRow("bin", "look", i.bin.bytes, i.bin.items, i.bin.drives.map((d) => ({ label: d.root, bytes: d.bytes }))));
  }

  // Good to know
  const guideRules = (i.junk?.rules ?? []).filter((r) => r.level === "protected" && r.tool && r.bytes > 0);
  const guideAdvice = (i.advice?.suggestions?.groups ?? []).filter((g) => g.verdict === "guide" && !(g.source && junkIds.has(g.source)));
  const guide: Detail[] = [
    ...guideRules.map((r) => ({ label: r.app, bytes: r.bytes, ruleId: r.ruleId })),
    ...guideAdvice.map(titleDetail),
  ];
  if (guide.length) rows.push(infoRow("guide", "info", sum(guide, (d) => d.bytes), guide.length, guide.sort((a, b) => b.bytes - a.bytes)));
  const admin = junkRules.filter((r) => r.admin && (r.level === "safe" || r.level === "caution"));
  if (admin.length) rows.push(infoRow("admin", "info", sum(admin, (r) => r.bytes), admin.length, admin.map(junkDetail)));

  return rows.filter((r): r is Row => r !== null && (r.count > 0 || r.bytes > 0));
}

/** Rows ticked when the result opens. */
export const defaultTicks = (rows: Row[]): RowId[] => rows.filter((r) => r.tickable && r.ticked).map((r) => r.id);

/** Rows that can actually be ticked (stale ids dropped). */
export const sanitizeTicks = (rows: Row[], ticks: Iterable<string>): RowId[] => {
  const set = new Set(ticks);
  return rows.filter((r) => r.tickable && set.has(r.id)).map((r) => r.id);
};

/** Rows whose sizes come from rule walks or scan suggestions: they may overlap on
 * disk (a folder two rules both match), so their sum is an upper bound. */
const PATH_SOURCES: RowId[] = ["junk", "junk-caution", "dev", "advice"];

export interface Totals {
  /** What the ticked rows free, without what open apps keep. */
  bytes: number;
  /** Ticked bytes likely to stay while apps are open. */
  skipBytes: number;
  count: number;
  rows: number;
  /** The figure is an upper bound (several rule walks ticked, or apps to close). */
  upTo: boolean;
  /** A Caution row is ticked: the confirmation needs the acknowledgement. */
  caution: boolean;
  running: string[];
}

/** Totals of the ticked rows. Only tickable rows count: what is "worth a look" or
 * "good to know" never adds to the figure on the Clean button. */
export function totals(rows: Row[], ticks: Iterable<string>): Totals {
  const set = new Set(ticks);
  const picked = rows.filter((r) => r.tickable && set.has(r.id));
  const sources = picked.filter((r) => PATH_SOURCES.includes(r.id)).length;
  const skipBytes = sum(picked, (r) => r.skipBytes);
  return {
    bytes: sum(picked, (r) => r.bytes) - skipBytes,
    skipBytes,
    count: sum(picked, (r) => r.count),
    rows: picked.length,
    upTo: sources > 1 || skipBytes > 0,
    caution: picked.some((r) => r.caution),
    running: uniq(picked.flatMap((r) => r.running)),
  };
}

// ── Running ──

export type Tool = "advice" | "tidy" | "junk" | "dev";

export interface Task {
  tool: Tool;
  ruleIds: string[];
  keys: string[];
  ids: number[];
  bytes: number;
}

/** What runs, in order: the scan's own suggestions and Tidy first (they act on the
 * entries of the scan, which a rule clean would make stale), then the rule cleans. */
export function runPlan(rows: Row[], ticks: Iterable<string>): Task[] {
  const set = new Set(ticks);
  const picked = rows.filter((r) => r.tickable && set.has(r.id));
  const of = (ids: RowId[]) => picked.filter((r) => ids.includes(r.id));
  const task = (tool: Tool, rs: Row[]): Task | null =>
    rs.length
      ? {
          tool,
          ruleIds: rs.flatMap((r) => r.ruleIds),
          keys: rs.flatMap((r) => r.keys),
          ids: rs.flatMap((r) => (tool === "tidy" ? r.ids : [])),
          bytes: sum(rs, (r) => r.bytes - r.skipBytes),
        }
      : null;
  return [task("advice", of(["advice"])), task("tidy", of(["empty", "links"])), task("junk", of(["junk", "junk-caution"])), task("dev", of(["dev"]))].filter(
    (t): t is Task => t !== null,
  );
}

export type PartStatus = "waiting" | "running" | "done" | "failed" | "cancelled";

/** How one tool's part of the clean went. */
export interface Part {
  tool: Tool;
  status: PartStatus;
  freed: number;
  done: number;
  /** Items that stayed (skipped, failed or blocked). */
  stayed: number;
  /** Why they stayed: reason code and how many. */
  reasons: { code: string; count: number }[];
  error: string | null;
}

export const waitingPart = (tool: Tool): Part => ({ tool, status: "waiting", freed: 0, done: 0, stayed: 0, reasons: [], error: null });

/** Reasons of the items an operation left, most frequent first. */
export function reasonsOf(r: Pick<OperationResult, "problems"> | null | undefined): { code: string; count: number }[] {
  const m = new Map<string, number>();
  for (const p of r?.problems ?? []) {
    const code = p.reason ?? p.status;
    m.set(code, (m.get(code) ?? 0) + 1);
  }
  return [...m.entries()].map(([code, count]) => ({ code, count })).sort((a, b) => b.count - a.count || a.code.localeCompare(b.code));
}

/** A tool's operation as a part of the result. */
export function partOf(tool: Tool, r: OperationResult | null, error: string | null = null): Part {
  if (!r) return { ...waitingPart(tool), status: error ? "failed" : "done", error };
  return {
    tool,
    status: r.cancelled ? "cancelled" : "done",
    freed: r.freed,
    done: r.done + r.partial,
    stayed: r.skipped + r.failed,
    reasons: reasonsOf(r),
    error,
  };
}

/** Freed space and leftovers of the whole clean. */
export function outcomeTotals(parts: Part[]): { freed: number; done: number; stayed: number; failed: number } {
  return {
    freed: sum(parts, (p) => p.freed),
    done: sum(parts, (p) => p.done),
    stayed: sum(parts, (p) => p.stayed),
    failed: parts.filter((p) => p.status === "failed").length,
  };
}
