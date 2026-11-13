// Quick clean: the step state machine, the result rows (no byte counted twice across
// tools, what is never pre-ticked), the totals on the Clean button, the order the
// tools run in, and a message in both languages for every key the view builds.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  STEP_ORDER,
  buildRows,
  canMove,
  cancelOpen,
  coveredRules,
  defaultTicks,
  insideAny,
  joinRoot,
  moveStep,
  outcomeTotals,
  partOf,
  planSteps,
  progressOf,
  quickAdvice,
  reasonsOf,
  runPlan,
  sanitizeTicks,
  scanPhase,
  skipDependents,
  totals,
  type Inputs,
  type RowId,
} from "./quick/model.ts";
import type { CleanScan, OperationResult, RuleSummary } from "./clean/types.ts";
import type { DevCacheReport, DevRule } from "./devcache/types.ts";
import type { AdviceGroup, AdviceSummary, Located } from "./types.ts";

const GB = 2 ** 30;
const MB = 2 ** 20;

function rule(ruleId: string, level: RuleSummary["level"], bytes: number, extra: Partial<RuleSummary> = {}): RuleSummary {
  return {
    ruleId,
    packId: "p",
    app: ruleId,
    vendor: "v",
    category: "cache",
    level,
    impact: "",
    impactVi: null,
    requiresClosed: [],
    running: [],
    bytes,
    files: 10,
    candidates: bytes > 0 ? 1 : 0,
    skipped: 0,
    refused: 0,
    truncated: false,
    ...extra,
  };
}

function junk(rules: RuleSummary[]): CleanScan {
  return { at: 1, rules, packs: [], safe: 0, caution: 0, advanced: 0 };
}

function devReport(rules: DevRule[]): DevCacheReport {
  return {
    vendors: [{ vendor: "V", bytes: 0, files: 0, safeBytes: 0, cautionBytes: 0, advancedBytes: 0, newest: 0, running: [], threshold: null, overThreshold: false, apps: [{ app: "A", bytes: 0, files: 0, safeBytes: 0, newest: 0, running: [], rules, kept: [] }] }],
    safe: 0,
    caution: 0,
    advanced: 0,
    kept: 0,
  };
}

function group(key: string, extra: Partial<AdviceGroup> = {}): AdviceGroup {
  return {
    key,
    verdict: "safe",
    reason: "build-output",
    level: "safe",
    source: "rust",
    app: null,
    title: null,
    impact: { en: "", vi: "" },
    untouched: null,
    action: "quarantine",
    blocked: null,
    store: null,
    requiresClosed: [],
    bytes: GB,
    recent: 0,
    files: 100,
    count: 1,
    selected: true,
    actionable: true,
    hard: false,
    overridable: false,
    location: "proj\\target",
    elsewhere: false,
    items: [{ id: 7, path: "proj\\target", bytes: GB, files: 100, blocked: null }],
    ...extra,
  };
}

function advice(groups: AdviceGroup[], elsewhere: AdviceGroup[] = []): AdviceSummary {
  return {
    computing: false,
    seq: 1,
    root: "C:\\",
    suggestions: { totals: { safe: 0, caution: 0, guide: 0, keep: 0, unknown: 0 }, reclaim: 0, groups, enriched: true },
    elsewhere,
  };
}

function node(id: number, path: string): Located {
  return { id, name: path.split("\\").pop() ?? "", kind: "dir", logical: 0, onDisk: 0, files: 0, mtime: 0, attrs: [], hasChildren: false, hardlinkDup: false, path };
}

const base: Inputs = { junk: null, dev: null, bin: null, root: "C:\\", empty: null, links: null, advice: null, large: null, dupes: null };

// ── steps ──

test("planSteps leaves out what the platform lacks", () => {
  const win = planSteps({ junk: true, bin: true, drive: true });
  assert.deepEqual(
    win.map((s) => s.id),
    STEP_ORDER,
  );
  assert.ok(win.every((s) => s.status === "waiting"));
  const mac = planSteps({ junk: false, bin: false, drive: true });
  assert.deepEqual(
    mac.map((s) => s.id),
    ["dev", "scan", "tidy", "advice", "large", "dupes"],
  );
});

test("without a system drive the scan and what reads it are skipped", () => {
  const steps = planSteps({ junk: true, bin: true, drive: false });
  for (const id of ["scan", "tidy", "advice", "large"]) {
    assert.equal(steps.find((s) => s.id === id)?.status, "skipped", id);
  }
  assert.equal(steps.find((s) => s.id === "dupes")?.status, "waiting");
});

test("step transitions follow the state machine", () => {
  assert.ok(canMove("waiting", "running"));
  assert.ok(canMove("running", "done"));
  assert.ok(canMove("running", "failed"));
  assert.ok(!canMove("done", "running"));
  assert.ok(!canMove("waiting", "done"));
  assert.ok(!canMove("failed", "done"));
  let steps = planSteps({ junk: true, bin: true, drive: true });
  assert.equal(scanPhase(steps), "idle");
  steps = moveStep(steps, "junk", "running");
  assert.equal(scanPhase(steps), "running");
  // an invalid move changes nothing (same array back)
  const same = moveStep(steps, "dev", "done");
  assert.equal(same, steps);
  steps = moveStep(steps, "junk", "done", { bytes: 5, count: 2 });
  assert.equal(steps[0].bytes, 5);
  // a done step cannot be moved again
  assert.equal(moveStep(steps, "junk", "failed"), steps);
});

test("a failed scan skips its dependents but not the others", () => {
  let steps = planSteps({ junk: true, bin: true, drive: true });
  steps = moveStep(steps, "scan", "running");
  steps = moveStep(steps, "scan", "failed", { error: "x" });
  steps = skipDependents(steps);
  assert.equal(steps.find((s) => s.id === "tidy")?.status, "skipped");
  assert.equal(steps.find((s) => s.id === "advice")?.note, "quick-note-needs-scan");
  assert.equal(steps.find((s) => s.id === "dupes")?.status, "waiting");
});

test("stop cancels what has not ended, keeps what has", () => {
  let steps = planSteps({ junk: true, bin: true, drive: true });
  steps = moveStep(moveStep(steps, "junk", "running"), "junk", "done");
  steps = moveStep(steps, "dev", "running");
  steps = cancelOpen(steps);
  assert.equal(steps[0].status, "done");
  assert.equal(steps[1].status, "cancelled");
  assert.ok(steps.slice(1).every((s) => s.status === "cancelled"));
  assert.equal(scanPhase(steps), "finished");
  assert.equal(progressOf(steps), 1);
});

// ── rows and dedupe ──

test("safe junk is ticked, caution junk is offered but never ticked", () => {
  const rows = buildRows({ ...base, junk: junk([rule("a", "safe", GB), rule("b", "caution", 2 * GB), rule("c", "advanced", 3 * GB)]) });
  const safe = rows.find((r) => r.id === "junk");
  const caution = rows.find((r) => r.id === "junk-caution");
  assert.equal(safe?.bytes, GB);
  assert.equal(safe?.group, "safe");
  assert.equal(caution?.group, "look");
  assert.ok(caution?.caution);
  assert.deepEqual(defaultTicks(rows), ["junk"]);
  // advanced rules stay in the Junk files view
  assert.ok(!rows.some((r) => r.ruleIds.includes("c")));
});

test("a dev rule the junk scan already has is counted once", () => {
  const dup = { ...rule("shared", "safe", GB), newest: 0 } as DevRule;
  const own = { ...rule("dev.only", "safe", 2 * GB), newest: 0 } as DevRule;
  const rows = buildRows({ ...base, junk: junk([rule("shared", "safe", GB)]), dev: devReport([dup, own]) });
  assert.deepEqual(rows.find((r) => r.id === "dev")?.ruleIds, ["dev.only"]);
  assert.equal(totals(rows, defaultTicks(rows)).bytes, 3 * GB);
});

test("advice junk a rule scan covers is not counted again", () => {
  const i: Inputs = {
    ...base,
    junk: junk([rule("windows.temp.user", "safe", GB)]),
    advice: advice(
      [group("rule-in-scan", { reason: "rule", source: "windows.temp.user", action: "clean" }), group("build", { bytes: 4 * GB })],
      [group("elsewhere|windows.temp.user", { reason: "rule", source: "windows.temp.user", action: "clean", elsewhere: true })],
    ),
  };
  assert.ok(coveredRules(i).has("windows.temp.user"));
  assert.deepEqual(
    quickAdvice(i).map((g) => g.key),
    ["build"],
  );
  const rows = buildRows(i);
  assert.equal(rows.find((r) => r.id === "advice")?.bytes, 4 * GB);
  // junk 1 GB + build output 4 GB, never the rule's bytes twice
  assert.equal(totals(rows, defaultTicks(rows)).bytes, 5 * GB);
});

test("advice junk of a rule no scan covered still counts", () => {
  const i: Inputs = { ...base, advice: advice([group("r", { reason: "rule", source: "mac.only", action: "clean" })]) };
  assert.deepEqual(
    quickAdvice(i).map((g) => g.key),
    ["r"],
  );
});

test("only quarantine actions, Safe and pre-selected advice is ticked", () => {
  const i: Inputs = {
    ...base,
    advice: advice([
      group("ok"),
      group("caution", { verdict: "caution", selected: false }),
      group("ai", { action: "ai-leftovers" }),
      group("bin", { action: "recycle-bin" }),
      group("blocked", { blocked: "dirty", actionable: false, selected: false }),
      group("notpicked", { selected: false }),
    ]),
  };
  assert.deepEqual(
    quickAdvice(i).map((g) => g.key),
    ["ok"],
  );
  const rows = buildRows(i);
  // the rest is worth a look in the disk map, not ticked
  const more = rows.find((r) => r.id === "more");
  assert.ok(more && !more.tickable);
  assert.ok(more.details.some((d) => d.group?.key === "caution"));
});

test("empty folders and broken shortcuts inside a ticked suggestion go with it", () => {
  const loc = (id: number, path: string) => node(id, path);
  const i: Inputs = {
    ...base,
    advice: advice([group("build")]),
    empty: { rows: [{ ...loc(1, "C:\\proj\\target\\empty"), nested: 0, clutter: 0 }, { ...loc(2, "C:\\Users\\me\\Old"), nested: 0, clutter: 0 }], matched: 2, totalDirs: 2 },
    links: {
      rows: [
        { ...loc(3, "C:\\Users\\me\\Desktop\\gone.lnk"), target: "C:\\x", status: "missing" },
        { ...loc(4, "C:\\Users\\me\\Desktop\\usb.lnk"), target: "E:\\x", status: "drive_missing" },
        { ...loc(5, "C:\\proj\\target\\a.lnk"), target: "C:\\y", status: "missing" },
      ],
      checked: 3,
      unchecked: 0,
      unreadable: 0,
    },
  };
  const rows = buildRows(i);
  assert.deepEqual(rows.find((r) => r.id === "empty")?.ids, [2]);
  // a shortcut to an unplugged drive is never pre-ticked
  assert.deepEqual(rows.find((r) => r.id === "links")?.ids, [3]);
});

test("worth a look and good to know never add to the Clean total", () => {
  const i: Inputs = {
    ...base,
    junk: junk([rule("a", "safe", GB), rule("g", "protected", 9 * GB, { tool: "system-protection" }), rule("adm", "safe", MB, { admin: true })]),
    bin: { bytes: 5 * GB, items: 10, drives: [{ root: "C:\\", bytes: 5 * GB, items: 10 }], settled: 0 },
    large: { large: { rows: [], matched: 0, matchedOnDisk: 0 }, stale: { rows: [], matched: 0, matchedOnDisk: 0 }, installers: { rows: [], matched: 0, matchedOnDisk: 0 }, distinctFiles: 3, distinctOnDisk: 20 * GB },
  };
  const rows = buildRows(i);
  const bin = rows.find((r) => r.id === "bin");
  assert.ok(bin && !bin.tickable && !bin.ticked, "the Recycle Bin is never pre-ticked");
  assert.equal(rows.find((r) => r.id === "guide")?.group, "info");
  assert.equal(rows.find((r) => r.id === "admin")?.group, "info");
  assert.equal(rows.find((r) => r.id === "large")?.group, "look");
  const all = rows.map((r) => r.id);
  // ticking everything still counts only what can be ticked
  assert.equal(totals(rows, all).bytes, GB + MB);
  assert.deepEqual(sanitizeTicks(rows, all), ["junk"]);
});

test("totals say 'up to' when sources may overlap or apps hold files", () => {
  const rows = buildRows({
    ...base,
    junk: junk([rule("a", "safe", GB), rule("chrome", "safe", 2 * GB, { running: ["chrome.exe"] })]),
    advice: advice([group("build")]),
  });
  const t1 = totals(rows, ["junk"] as RowId[]);
  assert.equal(t1.bytes, GB, "what an open app holds is left out");
  assert.equal(t1.skipBytes, 2 * GB);
  assert.ok(t1.upTo);
  assert.deepEqual(t1.running, ["chrome.exe"]);
  const t2 = totals(buildRows({ ...base, junk: junk([rule("a", "safe", GB)]) }), ["junk"]);
  assert.ok(!t2.upTo, "one rule walk alone is not an estimate");
  const t3 = totals(buildRows({ ...base, junk: junk([rule("a", "safe", GB)]), advice: advice([group("build")]) }), ["junk", "advice"]);
  assert.ok(t3.upTo, "junk and suggestions together may overlap");
});

test("a ticked caution row asks for the acknowledgement", () => {
  const rows = buildRows({ ...base, junk: junk([rule("a", "safe", GB), rule("b", "caution", GB)]) });
  assert.ok(!totals(rows, ["junk"]).caution);
  assert.ok(totals(rows, ["junk", "junk-caution"]).caution);
});

// ── running ──

test("the scan's own entries are cleaned before the rule walks", () => {
  const loc = (id: number, path: string) => ({ ...node(id, path), nested: 0, clutter: 0 });
  const dev = { ...rule("dev.only", "safe", GB), newest: 0 } as DevRule;
  const rows = buildRows({
    ...base,
    junk: junk([rule("a", "safe", GB), rule("b", "caution", GB)]),
    dev: devReport([dev]),
    advice: advice([group("build")]),
    empty: { rows: [loc(9, "C:\\e")], matched: 1, totalDirs: 1 },
  });
  const plan = runPlan(rows, ["junk", "junk-caution", "dev", "advice", "empty"]);
  assert.deepEqual(
    plan.map((p) => p.tool),
    ["advice", "tidy", "junk", "dev"],
  );
  assert.deepEqual(plan.find((p) => p.tool === "junk")?.ruleIds.sort(), ["a", "b"]);
  assert.deepEqual(plan.find((p) => p.tool === "tidy")?.ids, [9]);
  assert.deepEqual(plan.find((p) => p.tool === "advice")?.keys, ["build"]);
  assert.deepEqual(runPlan(rows, []), []);
});

test("parts and the outcome add up honestly", () => {
  const r: OperationResult = {
    id: "op",
    freed: 3 * GB,
    done: 4,
    partial: 1,
    skipped: 2,
    failed: 1,
    cancelled: false,
    journalError: null,
    problems: [
      { path: "a", status: "skipped", reason: "in-use" },
      { path: "b", status: "skipped", reason: "in-use" },
      { path: "c", status: "failed", reason: null },
    ],
  };
  const part = partOf("junk", r);
  assert.equal(part.done, 5);
  assert.equal(part.stayed, 3);
  assert.deepEqual(reasonsOf(r), [
    { code: "in-use", count: 2 },
    { code: "failed", count: 1 },
  ]);
  const failed = partOf("dev", null, "boom");
  assert.equal(failed.status, "failed");
  const sum = outcomeTotals([part, failed]);
  assert.equal(sum.freed, 3 * GB);
  assert.equal(sum.failed, 1);
});

test("paths compare without case and separators", () => {
  assert.equal(joinRoot("C:\\", "proj\\target"), "C:\\proj\\target");
  assert.equal(joinRoot("/", "Users/me"), "/Users/me");
  assert.equal(joinRoot("C:\\", "D:\\x"), "D:\\x");
  assert.ok(insideAny("c:/PROJ/target/x", ["C:\\proj\\target"]));
  assert.ok(insideAny("C:\\proj\\target", ["C:\\proj\\target\\"]));
  assert.ok(!insideAny("C:\\proj\\target2", ["C:\\proj\\target"]));
  assert.ok(!insideAny("C:\\x", [""]));
});

// ── locales ──

function keys(lang: string): Set<string> {
  const out = new Set<string>();
  for (const file of ["quick.ftl", "app.ftl", "clean.ftl", "recyclebin.ftl", "ui.ftl"]) {
    const text = readFileSync(new URL(`../../../../locales/${lang}/${file}`, import.meta.url), "utf8");
    for (const line of text.split(/\r?\n/)) {
      const m = /^([a-z][\w-]*)\s*=/.exec(line);
      if (m) out.add(m[1]);
    }
  }
  return out;
}

test("every key the Quick clean view builds exists in both languages", () => {
  const rowIds: RowId[] = ["junk", "dev", "advice", "empty", "links", "junk-caution", "dupes", "large", "more", "bin", "guide", "admin"];
  const needed = [
    ...STEP_ORDER.map((s) => `quick-step-${s}`),
    ...["waiting", "running", "skipped", "failed", "cancelled"].map((s) => `quick-status-${s}`),
    ...rowIds.flatMap((r) => [`quick-row-${r}`, `quick-row-${r}-sub`]),
    ...["safe", "look", "info"].flatMap((g) => [`quick-group-${g}`, `quick-group-${g}-sub`]),
    ...["advice", "tidy", "junk", "dev"].map((tool) => `quick-tool-${tool}`),
    "quick-note-no-drive",
    "quick-note-needs-scan",
    "quick-note-reused",
    "rb-empty",
    "rb-empty-hint",
    "rb-confirm-title",
    "rb-confirm-body",
    "clean-left-size",
    "clean-confirm-note",
    "delete-cancel",
    "nav-quick",
    "nav-tools",
    "nav-diskmap",
  ];
  for (const lang of ["en", "vi"]) {
    const have = keys(lang);
    const missing = needed.filter((k) => !have.has(k));
    assert.deepEqual(missing, [], `${lang}: ${missing.join(", ")}`);
  }
});

test("Quick clean texts use no long dashes", () => {
  for (const lang of ["en", "vi"]) {
    const text = readFileSync(new URL(`../../../../locales/${lang}/quick.ftl`, import.meta.url), "utf8");
    assert.ok(!/[—–]|--/.test(text), lang);
  }
});
