// Browser-only stand-in for the Clean / Activity commands (`pnpm dev` without Tauri):
// a few rules shaped like a real Windows PC, a quarantine that "restores", and the
// delete of hand-picked entries delegated to the main mock's `recycle`.

import type { CleanScan, OperationItem, OperationRow, RuleSummary } from "./types";

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

const ops: { row: OperationRow; items: OperationItem[] }[] = [];

function record(kind: string, items: OperationItem[]): OperationRow {
  const t = now();
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
    restorable: items.length,
  };
  ops.unshift({ row, items });
  return row;
}

export function mockDeleted(kind: string, items: { path: string; bytes: number; isDir: boolean }[], mode: OperationItem["mode"]): void {
  record(
    kind,
    items.map((x, n) => ({ n, path: x.path, ruleId: null, mode, isDir: x.isDir, bytes: x.bytes, files: 1, freed: x.bytes, status: "done", reason: null, stored: null, restored: false })),
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
      stored: `quarantine\\${n}`,
      restored: false,
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
  activity_list: ({ limit }) => ops.slice(0, limit).map((o) => ({ ...o.row })),
  activity_items: ({ id }) => {
    const o = ops.find((x) => x.row.id === id);
    if (!o) throw { code: "history", message: `unknown operation ${id}` };
    return { operation: { ...o.row }, items: o.items.map((i) => ({ ...i })) };
  },
  activity_restore: ({ id, items }) => {
    const o = ops.find((x) => x.row.id === id);
    if (!o) throw { code: "history", message: `unknown operation ${id}` };
    const out = o.items
      .filter((i) => !i.restored && (!items || items.includes(i.n)))
      .map((i) => {
        i.restored = true;
        return { n: i.n, path: i.path, status: "restored", message: null };
      });
    o.row.restorable = o.items.filter((i) => !i.restored).length;
    o.row.state = o.row.restorable ? "partly-restored" : "restored";
    if (o.row.kind === "clean") reset();
    return out;
  },
  activity_purge: ({ id }) => {
    const o = ops.find((x) => x.row.id === id);
    if (o) {
      o.row.state = "purged";
      o.row.restorable = 0;
    }
  },
  activity_forget: ({ id }) => {
    const i = ops.findIndex((x) => x.row.id === id);
    if (i >= 0) ops.splice(i, 1);
  },
};
