// Browser-only stand-in for the Clean / Activity commands (`pnpm dev` without Tauri):
// a few rules shaped like a real Windows PC, a quarantine that "restores", and the
// delete of hand-picked entries delegated to the main mock's `recycle`.

import { HELD_MODES, heldItem, restorableItem } from "./quarantine.ts";
import type { CleanScan, OperationItem, OperationRow, PurgeResult, QuarantineInfo, RuleSummary } from "./types";

const MB = 2 ** 20;
const GB = 2 ** 30;
const now = () => Math.floor(Date.now() / 1000);

function rule(
  ruleId: string,
  packId: string,
  app: string,
  vendor: string,
  level: RuleSummary["level"],
  bytes: number,
  files: number,
  extra: Partial<RuleSummary> = {},
): RuleSummary {
  return {
    ruleId,
    packId,
    app,
    vendor,
    category: packId,
    level,
    impact: "Rebuilt automatically; the app may start a little slower once.",
    impactVi: "Được tạo lại tự động; ứng dụng có thể mở chậm hơn một chút một lần.",
    requiresClosed: [],
    running: [],
    bytes,
    files,
    candidates: bytes ? Math.max(1, Math.round(files / 40)) : 0,
    skipped: 0,
    refused: 0,
    truncated: false,
    ...extra,
  };
}

let rules: RuleSummary[] = [];
function reset(): void {
  rules = [
    rule("windows.temp.user", "windows-system", "Windows", "Microsoft", "safe", 1.4 * GB, 3200),
    rule("windows.explorer.thumbcache", "windows-system", "Windows Explorer", "Microsoft", "safe", 820 * MB, 24, {
      requiresClosed: ["explorer.exe"],
      running: ["explorer.exe"],
    }),
    rule("windows.crashdumps.user", "windows-system", "Windows Error Reporting", "Microsoft", "safe", 276 * MB, 10),
    rule("windows.update.download-cache", "windows-system", "Windows", "Microsoft", "advanced", 2.1 * GB, 410),
    rule("windows.winsxs", "windows-system", "Windows", "Microsoft", "protected", 0, 0, {
      impact: "Component store. Reclaim space with DISM /Online /Cleanup-Image /StartComponentCleanup.",
      impactVi: "Kho thành phần. Thu hồi dung lượng bằng DISM /Online /Cleanup-Image /StartComponentCleanup.",
    }),
    rule("browser.chrome.cache", "browsers", "Chrome", "Google", "safe", 734 * MB, 21942, {
      requiresClosed: ["chrome.exe"],
    }),
    rule("browser.edge.cache", "browsers", "Edge", "Microsoft", "safe", 149 * MB, 2149, {
      requiresClosed: ["msedge.exe"],
      running: ["msedge.exe"],
    }),
    rule("gpu.nvidia.shader-cache", "gpu-shader-cache", "GPU driver", "NVIDIA", "safe", 101 * MB, 164),
    rule("anthropic.claude-code.file-history", "ai-tools", "Claude Code", "Anthropic", "caution", 380 * MB, 920),
  ];
}
reset();

const PACKS = [
  { id: "ai-tools", name: "AI tools" },
  { id: "browsers", name: "Web browser caches" },
  { id: "gpu-shader-cache", name: "GPU shader caches" },
  { id: "windows-system", name: "Windows system junk" },
];

function scan(): CleanScan {
  const sum = (level: string) => rules.filter((r) => r.level === level).reduce((s, r) => s + r.bytes, 0);
  return {
    at: now(),
    rules: rules.map((r) => ({ ...r })),
    packs: PACKS.map((p) => ({ ...p, bytes: rules.filter((r) => r.packId === p.id).reduce((s, r) => s + r.bytes, 0) })),
    safe: sum("safe"),
    caution: sum("caution"),
    advanced: sum("advanced"),
  };
}

type MockOp = { row: OperationRow; items: OperationItem[] };
const ops: MockOp[] = [];
const DAY = 86_400;
const TTL_DAYS = 7;
const LOCAL = "C:\\Users\\nano\\AppData\\Local";
const QUARANTINE_ROOT = `${LOCAL}\\vn.io.nanodev.nanofreedisk\\quarantine`;

/** Recount what an operation can still restore and what the quarantine holds of it. */
function derive(o: MockOp): void {
  const r = o.row;
  r.restorable = o.items.filter((i) => restorableItem(i, r.state)).length;
  const held = o.items.filter((i) => heldItem(i, r.state));
  r.quarantined = held.length;
  r.held = held.reduce((s, i) => s + i.freed, 0);
  r.expiresAt = held.some((i) => i.mode !== "registry") ? r.startedAt + TTL_DAYS * DAY : null;
}

function record(kind: string, items: OperationItem[], age = 0): OperationRow {
  const t = now() - age;
  const row: OperationRow = {
    id: `mock-${t}-${ops.length}`,
    kind,
    startedAt: t,
    finishedAt: t,
    items: items.length,
    done: items.length,
    bytes: items.reduce((s, i) => s + i.bytes, 0),
    freed: items.reduce((s, i) => s + i.freed, 0),
    cancelled: false,
    state: "active",
    restorable: 0,
    quarantined: 0,
    held: 0,
    expiresAt: null,
  };
  const o = { row, items };
  derive(o);
  ops.unshift(o);
  ops.sort((a, b) => b.row.startedAt - a.row.startedAt);
  return row;
}

function item(n: number, path: string, mode: OperationItem["mode"], bytes: number, isDir = true): OperationItem {
  const name = path.split("\\").pop();
  const stored =
    mode === "registry"
      ? `${LOCAL}\\vn.io.nanodev.nanofreedisk\\registry-backup\\seed\\${n}.reg`
      : HELD_MODES.includes(mode)
        ? `${QUARANTINE_ROOT}\\seed\\${n}\\${name}`
        : null;
  return { n, path, ruleId: null, mode, isDir, bytes, files: isDir ? 40 : 1, freed: bytes, status: "done", reason: null, stored, restored: false, message: null, purged: false };
}

/** A few operations from the past days, so the Activity log and its quarantine
 *  have something to show in a browser preview. */
function seed(): void {
  record(
    "clean",
    [
      item(0, `${LOCAL}\\Google\\Chrome\\User Data\\Default\\Cache`, "quarantine", 1.4 * GB),
      item(1, `${LOCAL}\\Temp`, "quarantine", 860 * MB),
      item(2, `${LOCAL}\\npm-cache\\_cacache`, "quarantine", 2.1 * GB),
    ],
    2 * DAY,
  );
  record(
    "uninstall",
    [
      item(0, `${LOCAL}\\Programs\\Old Chat`, "quarantine", 310 * MB),
      item(1, "HKEY_CURRENT_USER\\Software\\Old Chat", "registry", 0, false),
      item(2, "C:\\ProgramData\\Old Chat", "recycle-bin", 45 * MB),
    ],
    DAY,
  );
  record("manual", [item(0, "C:\\Users\\nano\\Downloads\\setup-old.exe", "recycle-bin", 120 * MB, false)], 5 * DAY);
}
seed();

/** Delete held items of `o` for good (all, or `only`). */
function purgeOp(o: MockOp, only?: number[] | null): PurgeResult {
  const out: PurgeResult = { items: 0, bytes: 0, failed: [] };
  for (const i of o.items.filter((x) => heldItem(x, o.row.state) && (!only || only.includes(x.n)))) {
    i.purged = true;
    out.items++;
    out.bytes += i.freed;
  }
  derive(o);
  if (!o.row.restorable && o.items.some((i) => i.purged)) o.row.state = "purged";
  derive(o);
  return out;
}

export function mockDeleted(kind: string, items: { path: string; bytes: number; isDir: boolean }[], mode: OperationItem["mode"]): void {
  record(
    kind,
    items.map((x, n) => ({ n, path: x.path, ruleId: null, mode, isDir: x.isDir, bytes: x.bytes, files: 1, freed: x.bytes, status: "done", reason: null, stored: null, restored: false, message: null, purged: false })),
  );
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const cleanHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  clean_scan: () => scan(),
  clean_items: ({ ruleId, limit }) => {
    const r = rules.find((x) => x.ruleId === ruleId);
    if (!r) return [];
    return Array.from({ length: Math.min(limit, r.candidates) }, (_, i) => ({
      path: `C:\\Users\\nano\\AppData\\Local\\${r.app.replace(/\W+/g, "")}\\Cache\\item-${i}`,
      isDir: i % 3 === 0,
      bytes: Math.round(r.bytes / (i + 2)),
      files: 1 + (i % 7),
      newest: now() - 86_400 * (i + 2),
    }));
  },
  clean_run: ({ ruleIds }) => {
    const picked = rules.filter((r) => ruleIds.includes(r.ruleId) && r.level !== "protected");
    const items: OperationItem[] = picked.map((r, n) => ({
      n,
      path: `C:\\Users\\nano\\AppData\\Local\\${r.app.replace(/\W+/g, "")}\\Cache`,
      ruleId: r.ruleId,
      mode: "quarantine",
      isDir: true,
      bytes: r.bytes,
      files: r.files,
      freed: r.running.length ? Math.round(r.bytes * 0.8) : r.bytes,
      status: r.running.length ? "partial" : "done",
      reason: r.running.length ? "in-use" : null,
      stored: `${QUARANTINE_ROOT}\\new\\${n}\\Cache`,
      restored: false,
      message: null,
      purged: false,
    }));
    const row = record("clean", items);
    for (const r of picked) {
      r.bytes = r.running.length ? Math.round(r.bytes * 0.2) : 0;
      r.files = r.bytes ? Math.round(r.files * 0.2) : 0;
      r.candidates = r.bytes ? 1 : 0;
    }
    const partial = items.filter((i) => i.status === "partial");
    return {
      id: row.id,
      freed: row.freed,
      done: items.length - partial.length,
      partial: partial.length,
      skipped: 0,
      failed: 0,
      cancelled: false,
      problems: partial.map((i) => ({ path: i.path, status: "partial", reason: "in-use" })),
      journalError: null,
    };
  },
  clean_cancel: () => undefined,
  activity_list: ({ limit }) =>
    ops.slice(0, limit).map((o) => {
      derive(o);
      return { ...o.row };
    }),
  activity_items: ({ id }) => {
    const o = ops.find((x) => x.row.id === id);
    if (!o) throw { code: "history", message: `unknown operation ${id}` };
    return { operation: { ...o.row }, items: o.items.map((i) => ({ ...i })) };
  },
  activity_restore: ({ id, items }) => {
    const o = ops.find((x) => x.row.id === id);
    if (!o) throw { code: "history", message: `unknown operation ${id}` };
    const out = o.items
      .filter((i) => restorableItem(i, o.row.state) && (!items || items.includes(i.n)))
      .map((i) => {
        i.restored = true;
        return { n: i.n, path: i.path, status: "restored", message: null };
      });
    derive(o);
    o.row.state = o.row.restorable ? "partly-restored" : "restored";
    derive(o);
    if (o.row.kind === "clean") reset();
    return out;
  },
  activity_purge: ({ id, items }) => {
    const o = ops.find((x) => x.row.id === id);
    if (!o) throw { code: "history", message: `unknown operation ${id}` };
    return purgeOp(o, items);
  },
  quarantine_summary: (): QuarantineInfo => {
    const held = ops.flatMap((o) => o.items.filter((i) => heldItem(i, o.row.state)).map((i) => ({ i, o })));
    const files = held.filter((h) => h.i.mode !== "registry").map((h) => h.o.row.startedAt);
    return {
      items: held.length,
      bytes: held.reduce((s, h) => s + h.i.freed, 0),
      operations: new Set(held.map((h) => h.o.row.id)).size,
      nextExpiry: files.length ? Math.min(...files) + TTL_DAYS * DAY : null,
      ttlDays: TTL_DAYS,
      roots: held.length ? [QUARANTINE_ROOT] : [],
    };
  },
  quarantine_empty: (): PurgeResult => {
    const out: PurgeResult = { items: 0, bytes: 0, failed: [] };
    for (const o of ops) {
      const r = purgeOp(o);
      out.items += r.items;
      out.bytes += r.bytes;
    }
    return out;
  },
  quarantine_reveal: () => undefined,
  activity_forget: ({ id }) => {
    const i = ops.findIndex((x) => x.row.id === id);
    if (i >= 0) ops.splice(i, 1);
  },
};
