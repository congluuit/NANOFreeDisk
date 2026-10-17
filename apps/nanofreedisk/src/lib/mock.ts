// In-memory stand-in for the Rust backend, used when the UI runs in a plain browser
// (`pnpm dev` without Tauri): a deterministic synthetic drive of ~80k entries, enough
// to exercise the treemap at the Phase 2 target (50k rectangles). Never bundled into
// the Tauri build's hot path — `api.ts` imports it lazily only outside Tauri.

import { browsersHandlers } from "./browsers/mock";
import { cleanHandlers, mockDeleted } from "./clean/mock";
import { devHandlers } from "./devcache/mock";
import { dupeHandlers } from "./dupes/mock";
import { healthHandlers } from "./health/mock";
import type { AdviceReason, DeleteItem, Filter, Node, NodeAdvice, Slice } from "./types";
import { REST_ID } from "./types";
import { hwHandlers } from "./hw/mock";
import { monitorHandlers } from "./monitor/mock";

interface E {
  id: number;
  parent: number;
  name: string;
  kind: Node["kind"];
  size: number;
  mtime: number;
  attrs: number;
  removed: boolean;
}

let seed = 42;
function rnd(): number {
  // mulberry32
  seed |= 0;
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const pick = <T>(xs: T[]): T => xs[Math.floor(rnd() * xs.length)];

const EXTS = [
  "mp4", "mkv", "jpg", "png", "raw", "mp3", "flac", "zip", "7z", "iso", "rs", "ts", "js",
  "json", "cpp", "h", "py", "exe", "dll", "pdb", "obj", "pdf", "docx", "xlsx", "txt",
  "log", "db", "vhdx", "pak", "bin", "dat", "uasset", "",
];
const WORDS = [
  "cache", "data", "assets", "build", "src", "logs", "media", "backup", "project", "temp",
  "shader", "models", "library", "content", "plugins", "packages", "release", "debug",
  "archive", "photos", "renders", "textures", "sessions", "snapshots", "config", "modules",
];

const entries: E[] = [];
const kids: number[][] = [];
let rolled: { size: number; files: number }[] = [];

function add(parent: number, name: string, kind: Node["kind"], size = 0, attrs = 0): number {
  const id = entries.length;
  entries.push({
    id,
    parent,
    name,
    kind,
    size,
    mtime: 1_600_000_000 + Math.floor(rnd() * 190_000_000),
    attrs,
    removed: false,
  });
  kids.push([]);
  if (parent >= 0) kids[parent].push(id);
  return id;
}

function fill(dir: number, depth: number, budget: { left: number }, scale: number): void {
  const nDirs = depth > 6 ? 0 : Math.floor(rnd() * (depth < 2 ? 9 : 5));
  const nFiles = Math.floor(rnd() * (depth < 2 ? 12 : 30)) + 1;
  for (let i = 0; i < nFiles && budget.left > 0; i++, budget.left--) {
    const ext = pick(EXTS);
    const size = Math.floor(Math.exp(rnd() * 14 + 4) * scale * (ext === "mkv" || ext === "vhdx" || ext === "iso" ? 40 : 1));
    add(dir, `${pick(WORDS)}_${i}${ext ? "." + ext : ""}`, "file", size, rnd() < 0.05 ? 2 : 0);
  }
  for (let i = 0; i < nDirs && budget.left > 0; i++, budget.left--) {
    const d = add(dir, `${pick(WORDS)}${i ? "-" + i : ""}`, "dir");
    fill(d, depth + 1, budget, scale);
  }
}

function build(): void {
  seed = 42;
  entries.length = 0;
  kids.length = 0;
  const root = add(-1, "C:\\", "dir");
  const named = (parent: number, path: string[]): number => {
    let cur = parent;
    for (const n of path) {
      const found = kids[cur].find((k) => entries[k].name === n);
      cur = found ?? add(cur, n, "dir");
    }
    return cur;
  };
  const budget = { left: 80_000 };
  fill(named(root, ["Windows"]), 1, { left: 14_000 }, 1);
  fill(named(root, ["Program Files"]), 1, { left: 9_000 }, 2);
  fill(named(root, ["Users", "nano", "AppData", "Local"]), 2, { left: 16_000 }, 1);
  fill(named(root, ["Users", "nano", "Downloads"]), 3, { left: 3_000 }, 6);
  fill(named(root, ["Users", "nano", "Videos"]), 3, { left: 1_500 }, 30);
  fill(named(root, ["Users", "nano", ".cargo", "registry"]), 4, { left: 9_000 }, 1);
  fill(named(root, ["Games", "SteamLibrary"]), 1, { left: 8_000 }, 12);
  fill(named(root, ["Projects"]), 1, budget, 1);
  // A3 material: empty folder trees and shortcuts (see broken_shortcuts below)
  const docs = named(root, ["Users", "nano", "Documents"]);
  named(docs, ["Old Project", "assets", "empty"]);
  named(docs, ["New folder"]);
  named(root, ["Users", "nano", "AppData", "Local", "Temp", "~build-cache"]);
  const desk = named(root, ["Users", "nano", "Desktop"]);
  for (const n of ["Photoshop.lnk", "Old Game.lnk", "Notes.lnk", "Tax 2019.lnk"]) add(desk, n, "file", 2048);
  add(root, "pagefile.sys", "file", 17_179_869_184, 3);
  add(root, "hiberfil.sys", "file", 6_871_947_673, 3);
  add(named(root, ["Users", "nano"]), "AppData-link", "junction");
  rollUp();
  units = [];
}

function rollUp(): void {
  rolled = entries.map((e) => ({
    size: e.kind === "file" && !e.removed ? e.size : 0,
    files: e.kind === "file" && !e.removed ? 1 : 0,
  }));
  // Subtrees are not contiguous in this builder (unlike the Rust arena), so sizes
  // are rolled up along a DFS order instead of index ranges.
  const order: number[] = [];
  const stack = [0];
  while (stack.length) {
    const n = stack.pop()!;
    order.push(n);
    for (const k of kids[n]) stack.push(k);
  }
  for (let i = order.length - 1; i >= 0; i--) {
    const n = order[i];
    const p = entries[n].parent;
    if (p >= 0 && !entries[n].removed) {
      rolled[p].size += rolled[n].size;
      rolled[p].files += rolled[n].files;
    }
  }
}

function descendants(id: number): number[] {
  const out: number[] = [];
  const stack = [...kids[id]];
  while (stack.length) {
    const n = stack.pop()!;
    if (entries[n].removed) continue;
    out.push(n);
    stack.push(...kids[n]);
  }
  return out;
}

const ATTR_NAMES = ["system", "hidden", "reparse", "compressed", "sparse", "offline", "readonly"];

function node(id: number): Node {
  const e = entries[id];
  const live = kids[id].filter((k) => !entries[k].removed);
  return {
    id,
    name: e.name,
    kind: e.kind,
    logical: rolled[id].size,
    onDisk: rolled[id].size,
    files: rolled[id].files,
    mtime: e.mtime,
    attrs: ATTR_NAMES.filter((_, i) => e.attrs & (1 << i)),
    hasChildren: e.kind === "dir" && live.length > 0,
    hardlinkDup: false,
    advice: adviceNode(id),
  };
}

function path(id: number): string {
  const parts: string[] = [];
  let cur = id;
  while (cur > 0) {
    parts.push(entries[cur].name);
    cur = entries[cur].parent;
  }
  return "C:\\" + parts.reverse().join("\\");
}

const located = (id: number) => ({ ...node(id), path: path(id) });

function sortedKids(id: number): number[] {
  return kids[id]
    .filter((k) => !entries[k].removed)
    .sort((a, b) => rolled[b].size - rolled[a].size || entries[a].name.localeCompare(entries[b].name));
}

function slice(id: number, maxDepth: number, minBytes: number, maxNodes: number): Slice {
  const s: Slice = { ids: [], parents: [], sizes: [], kinds: [], names: [], depths: [], truncated: false };
  const kindCode = (k: Node["kind"]) => (k === "dir" ? 1 : k === "file" || k === "other" ? 0 : 2);
  const push = (i: number, p: number, size: number, k: number, name: string, d: number) => {
    s.ids.push(i);
    s.parents.push(p);
    s.sizes.push(size);
    s.kinds.push(k);
    s.names.push(name);
    s.depths.push(d);
  };
  push(id, -1, rolled[id].size, kindCode(entries[id].kind), entries[id].name, 0);
  const queue: [number, number, number][] = [[id, 0, 0]];
  for (let q = 0; q < queue.length; q++) {
    const [dir, index, depth] = queue[q];
    if (depth >= maxDepth || entries[dir].kind !== "dir") continue;
    let rest = 0;
    for (const k of sortedKids(dir)) {
      const size = rolled[k].size;
      if (size === 0) continue;
      if (size < minBytes || s.ids.length >= maxNodes) {
        if (size >= minBytes) s.truncated = true;
        rest += size;
        continue;
      }
      const ci = s.ids.length;
      push(k, index, size, kindCode(entries[k].kind), entries[k].name, depth + 1);
      if (entries[k].kind === "dir") queue.push([k, ci, depth + 1]);
    }
    if (rest > 0) push(REST_ID, index, rest, 3, "", depth + 1);
  }
  if (!units.length) buildAdvice();
  s.verdicts = s.ids.map((i) => (i === REST_ID || unitOf[i] < 0 ? 255 : VERDICT_ORDER.indexOf(units[unitOf[i]].verdict)));
  return s;
}

function matches(id: number, f: Filter): boolean {
  const e = entries[id];
  if (f.kind === "files" && e.kind !== "file") return false;
  if (f.kind === "dirs" && e.kind !== "dir") return false;
  const size = rolled[id].size;
  if (f.min_size != null && size < f.min_size) return false;
  if (f.max_size != null && size > f.max_size) return false;
  if (f.modified_after != null && e.mtime < f.modified_after) return false;
  if (f.modified_before != null && e.mtime > f.modified_before) return false;
  if (f.attrs_none && e.attrs & f.attrs_none) return false;
  if (f.attrs_all && (e.attrs & f.attrs_all) !== f.attrs_all) return false;
  if (f.exts?.length) {
    if (e.kind !== "file") return false;
    const dot = e.name.lastIndexOf(".");
    const ext = dot > 0 ? e.name.slice(dot + 1).toLowerCase() : "";
    if (!f.exts.includes(ext)) return false;
  }
  if (f.name?.trim()) {
    const pat = f.name.trim().toLowerCase();
    const name = e.name.toLowerCase();
    if (/[*?]/.test(pat)) {
      const re = new RegExp("^" + pat.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".") + "$");
      if (!re.test(name)) return false;
    } else if (!name.includes(pat)) return false;
  }
  return true;
}

let scanTimer: ReturnType<typeof setInterval> | undefined;
const listeners = new Map<string, Set<(p: unknown) => void>>();
const emit = (event: string, payload: unknown) => listeners.get(event)?.forEach((cb) => cb(payload));

const PROTECTED_TOP = ["windows", "program files", "program files (x86)", "programdata", "pagefile.sys", "hiberfil.sys"];

function check(id: number): DeleteItem {
  const n = node(id);
  const top = path(id).split("\\")[1]?.toLowerCase() ?? "";
  const p = path(id).toLowerCase();
  let reason: DeleteItem["reason"] = null;
  if (entries[id].kind === "junction") reason = "link";
  else if (top === "windows") reason = "system-dir";
  else if (PROTECTED_TOP.includes(top)) reason = "protected";
  else if (["c:\\users", "c:\\users\\nano", "c:\\users\\nano\\appdata", "c:\\users\\nano\\appdata\\local", "c:\\users\\nano\\downloads", "c:\\users\\nano\\videos", "c:\\users\\nano\\documents", "c:\\users\\nano\\desktop", "c:\\users\\nano\\appdata\\local\\temp"].includes(p))
    reason = "contains-protected";
  return { id, name: n.name, path: path(id), kind: n.kind, onDisk: n.onDisk, files: n.files, allowed: !reason, reason, template: reason === "protected" ? `<volume>/${top}` : reason === "contains-protected" ? "${HOME}/Documents" : null };
}

const INSTALLERS = ["exe", "msi", "msix", "msixbundle", "appx", "iso", "img", "zip", "7z", "rar"];
const MOCK_TARGETS: Record<string, [string, "ok" | "missing" | "drive_missing"]> = {
  "Photoshop.lnk": ["G:\\Adobe\\Photoshop.exe", "drive_missing"],
  "Old Game.lnk": ["C:\\Games\\OldGame\\game.exe", "missing"],
  "Notes.lnk": ["C:\\Users\\nano\\Documents\\notes.txt", "ok"],
  "Tax 2019.lnk": ["C:\\Users\\nano\\Documents\\Tax 2019.xlsx", "missing"],
};

function ranked(ids: number[], limit: number) {
  ids.sort((a, b) => rolled[b].size - rolled[a].size || a - b);
  return { rows: ids.slice(0, limit).map(located), matched: ids.length, matchedOnDisk: ids.reduce((s, i) => s + rolled[i].size, 0) };
}

/** Folders whose subtree holds nothing but folders. */
/** Folders holding nothing but such folders; like the Rust side, folders the safety
 * check refuses are never listed but count as content for their parent. */
function emptyTrees(): (id: number) => boolean {
  const memo = new Map<number, boolean>();
  const isEmpty = (id: number): boolean => {
    let v = memo.get(id);
    if (v === undefined) {
      v =
        entries[id].kind === "dir" &&
        !check(id).reason &&
        kids[id].filter((k) => !entries[k].removed).every(isEmpty);
      memo.set(id, v);
    }
    return v;
  };
  return isEmpty;
}

// Scan history (A1/F1): three snapshots of C:\ a week apart; the diff is derived
// from the synthetic tree so every path in it can be opened.
let historyCleared = false;
let shellMenu = false;
let historyIds = [1, 2, 3];
const DAY = 86_400;

function snapshotRow(id: number) {
  const now = Math.floor(Date.now() / 1000);
  const age = (historyIds[historyIds.length - 1] - id) * 7 * DAY;
  const shrink = 1 - (historyIds[historyIds.length - 1] - id) * 0.01;
  return {
    id, root: "C:\\", volume: "c:\\", startedAt: now - age, elapsedMs: 1480,
    files: Math.round(rolled[0].files * shrink), dirs: 1392, onDisk: Math.round(rolled[0].size * shrink),
    volumeTotal: 4e12, volumeFree: 1.45e12 + (historyIds[historyIds.length - 1] - id) * 3.2e10, issues: 3,
    dirFloor: 2 ** 20, fileFloor: 10 * 2 ** 20,
  };
}

const rel = (id: number) => path(id).slice(3);

function mockDiff(older: number, newer: number) {
  const a = snapshotRow(older);
  const b = snapshotRow(newer);
  const weeks = Math.max(1, (newer - older) || 1);
  const dirs = descendants(0).filter((i) => entries[i].kind === "dir" && rolled[i].size > 5e9);
  dirs.sort((x, y) => rolled[y].size - rolled[x].size);
  const deep = dirs.filter((i) => path(i).split("\\").length > 4);
  const grew = deep.slice(0, 6).map((i, k) => {
    const delta = Math.round(rolled[i].size * (0.12 - k * 0.015) * weeks);
    return { path: rel(i), depth: path(i).split("\\").length - 2, before: rolled[i].size - delta, after: rolled[i].size, delta, filesDelta: 40 - k * 5 };
  });
  const shrank = deep.slice(6, 8).map((i, k) => {
    const delta = -Math.round(rolled[i].size * (0.2 - k * 0.05));
    return { path: rel(i), depth: path(i).split("\\").length - 2, before: rolled[i].size - delta, after: rolled[i].size, delta, filesDelta: -12 };
  });
  const big = descendants(0).filter((i) => entries[i].kind === "file").sort((x, y) => rolled[y].size - rolled[x].size);
  const filesUp = big.slice(0, 4).map((i) => ({ path: rel(i), before: 0, after: rolled[i].size, delta: rolled[i].size, mtime: entries[i].mtime }));
  const filesDown = [{ path: "Users\\nano\\Downloads\\Win11_23H2.iso", before: 6.6e9, after: 0, delta: -6.6e9, mtime: 1_700_000_000 }];
  return {
    older: a, newer: b, onDiskDelta: b.onDisk - a.onDisk, filesDelta: b.files - a.files,
    freeDelta: b.volumeFree - a.volumeFree, grew, shrank, filesUp, filesDown,
  };
}

const MOCK_ARTICLES = [
  {
    id: "pagefile", risk: "protected", path: "C:\\pagefile.sys", title: "Page file (virtual memory)",
    body: "Disk space Windows uses as extra memory when RAM is full.\n\n**If you delete it:** programs can crash with `out of memory` errors.\n\n**Instead:** move or resize it under Virtual memory.",
  },
  {
    id: "cargo-registry", risk: "caution", path: "C:\\Users\\nano\\.cargo\\registry", title: "Cargo registry and git cache (Rust)",
    body: "Downloaded crates for every Rust project you built.\n\n**If you delete it:** the next build downloads everything again.\n\n**Instead:** `cargo cache --autoclean`.",
  },
  {
    id: "windows-folder", risk: "protected", path: "C:\\Windows", title: "Windows",
    body: "The operating system itself.\n\n**If you delete anything here:** Windows may stop working. Use Disk Cleanup instead.",
  },
];


// ── Advice (Analyze → Suggestions): a few verdicts by path, like nfd-advice ──────

type MVerdict = "safe" | "caution" | "guide" | "keep" | "unknown";
interface MUnit {
  entry: number;
  verdict: MVerdict;
  reason: AdviceReason;
  level: string | null;
  source: string | null;
  app: string | null;
  title: { en: string; vi: string } | null;
  impact: { en: string; vi: string };
  untouched: { en: string; vi: string } | null;
  action: string;
  blocked: string | null;
  selected: boolean;
  store: string | null;
  requiresClosed: string[];
  cutoff: number | null;
}
const VERDICT_ORDER: MVerdict[] = ["safe", "caution", "guide", "keep", "unknown"];
const bi = (en: string, vi: string) => ({ en, vi });
let units: MUnit[] = [];
let unitOf: number[] = [];

function mockUnit(id: number, verdict: MVerdict, reason: AdviceReason, action: string, impact: { en: string; vi: string }, extra: Partial<MUnit> = {}): MUnit {
  return {
    entry: id, verdict, reason, level: verdict === "safe" ? "safe" : verdict === "caution" ? "caution" : null, source: null, app: null,
    title: null, impact, untouched: null, action, blocked: null, selected: verdict === "safe" && action !== "recycle-bin",
    store: null, requiresClosed: [], cutoff: null, ...extra,
  };
}

function classifyMock(id: number): MUnit | null {
  const p = path(id).toLowerCase();
  const e = entries[id];
  const name = e.name.toLowerCase();
  const parent = e.parent >= 0 ? entries[e.parent].name.toLowerCase() : "";
  if (p === "c:\\windows") return mockUnit(id, "keep", "system", "none", bi("Part of Windows. Deleting files here breaks Windows or its updates.", "Một phần của Windows. Xóa tệp ở đây làm hỏng Windows hoặc bản cập nhật."));
  if (p === "c:\\program files") return mockUnit(id, "keep", "programs", "none", bi("Installed programs: uninstall them from Settings → Apps instead.", "Chương trình đã cài: hãy gỡ trong Cài đặt → Ứng dụng."));
  if (name === "pagefile.sys" || name === "hiberfil.sys")
    return mockUnit(id, "guide", "explained", "guide", bi("Windows' memory on disk; only Windows' own settings shrink it.", "Bộ nhớ của Windows trên đĩa; chỉ thiết lập của Windows thu nhỏ được."), { title: name === "pagefile.sys" ? bi("Page file", "Tệp hoán đổi") : bi("Hibernation file", "Tệp ngủ đông"), source: name.replace(".sys", "") });
  if (parent === "temp" && p.includes("\\appdata\\local\\temp\\"))
    return mockUnit(id, "safe", "rule", "clean", bi("Per-user temporary files. Files newer than 24 hours are skipped.", "Tệp tạm của người dùng. Tệp mới hơn 24 giờ được bỏ qua."), { app: "Windows", source: "windows.temp.user", untouched: bi("Only what the rule covers goes, into the quarantine (restorable for 7 days).", "Chỉ những gì quy tắc bao phủ bị dọn, vào khu cách ly (khôi phục được trong 7 ngày).") });
  if (p.includes("\\.cargo\\registry") && parent === "registry")
    return mockUnit(id, "caution", "rule", "clean", bi("Downloaded crates; every project downloads them again on the next build.", "Crate đã tải; mọi dự án sẽ tải lại ở lần build sau."), { app: "Cargo", source: "rust.cargo.registry" });
  if (name === "videos" && parent === "nano") return mockUnit(id, "unknown", "personal", "none", bi("Your own photos, videos or music. Only you know what is worth keeping.", "Ảnh, video hoặc nhạc của riêng bạn. Chỉ bạn biết cái gì đáng giữ."));
  if (p.includes("\\downloads\\") && e.kind === "file" && /\.(iso|exe|msi|zip)$/.test(name))
    return mockUnit(id, "caution", "old-installer", "recycle-bin", bi("An installer or disk image left in Downloads for more than 30 days.", "Bộ cài hoặc ảnh đĩa nằm trong Downloads hơn 30 ngày."), { untouched: bi("It goes to the Recycle Bin: you can restore it from there.", "Tệp được chuyển vào Thùng rác: có thể khôi phục từ đó.") });
  if (p.startsWith("c:\\projects\\") && e.kind === "dir" && (name.startsWith("build") || name.startsWith("debug") || name.startsWith("release")))
    return mockUnit(id, name.startsWith("release") ? "caution" : "safe", "build-output", "quarantine", bi("Rust build output (`target`): the next `cargo build` creates it again.", "Kết quả build của Rust (`target`): lần `cargo build` sau sẽ tạo lại."), { source: "rust", untouched: bi("Source code and git history stay; only the build folder goes, into the quarantine.", "Mã nguồn và lịch sử git được giữ; chỉ thư mục build bị dọn, vào khu cách ly.") });
  return null;
}

function buildAdvice(): void {
  units = [];
  unitOf = new Array(entries.length).fill(-1);
  const order: number[] = [];
  const stack = [0];
  while (stack.length) {
    const n = stack.pop()!;
    order.push(n);
    for (const k of kids[n]) stack.push(k);
  }
  for (const id of order) {
    if (entries[id].removed) continue;
    const own = classifyMock(id);
    if (own) {
      units.push(own);
      unitOf[id] = units.length - 1;
    } else {
      const p = entries[id].parent;
      unitOf[id] = p >= 0 ? unitOf[p] : -1;
    }
  }
}

function verdictTotals(id: number): Record<MVerdict, number> {
  const t: Record<MVerdict, number> = { safe: 0, caution: 0, guide: 0, keep: 0, unknown: 0 };
  const ids = entries[id].kind === "file" ? [id] : descendants(id);
  for (const d of ids) {
    if (entries[d].kind !== "file" || entries[d].removed) continue;
    const u = unitOf[d];
    t[u >= 0 ? units[u].verdict : "unknown"] += entries[d].size;
  }
  return t;
}

function adviceNode(id: number): NodeAdvice {
  if (!units.length) buildAdvice();
  const u = unitOf[id] >= 0 ? units[unitOf[id]] : null;
  const inside = verdictTotals(id);
  const actionable = (x: MUnit) => !x.blocked && (x.verdict === "safe" || x.verdict === "caution") && ["clean", "quarantine", "ai-leftovers", "recycle-bin"].includes(x.action);
  let reclaim = 0;
  const ids = entries[id].kind === "file" ? [id] : descendants(id);
  for (const d of ids) {
    const x = unitOf[d] >= 0 ? units[unitOf[d]] : null;
    if (x && entries[d].kind === "file" && !entries[d].removed && actionable(x)) reclaim += entries[d].size;
  }
  const hard = !!u && (u.reason === "system" || u.reason === "programs");
  return { verdict: u?.verdict ?? null, reason: u?.reason ?? null, root: !!u && u.entry === id, hard, inside, reclaim };
}

function mockGroups() {
  if (!units.length) buildAdvice();
  const by = new Map<string, number[]>();
  units.forEach((u, i) => {
    if (entries[u.entry].removed) return;
    const split = (["system", "programs", "personal", "explained"] as string[]).includes(u.reason) ? `|${u.entry}` : "";
    const key = `${u.verdict}|${u.reason}|${u.source ?? ""}${split}`;
    by.set(key, [...(by.get(key) ?? []), i]);
  });
  const groups = [...by.entries()].map(([key, members]) => {
    const first = units[members[0]];
    const sized = members.map((i) => ({ i, bytes: verdictTotals(units[i].entry)[first.verdict], files: rolled[units[i].entry].files })).sort((a, b) => b.bytes - a.bytes);
    const bytes = sized.reduce((s, x) => s + x.bytes, 0);
    const actionable = !first.blocked && (first.verdict === "safe" || first.verdict === "caution") && ["clean", "quarantine", "ai-leftovers", "recycle-bin"].includes(first.action);
    return {
      key, verdict: first.verdict, reason: first.reason, level: first.level, source: first.source, app: first.app, title: first.title,
      impact: first.impact, untouched: first.untouched, action: first.action, blocked: first.blocked, store: first.store, requiresClosed: [],
      bytes, recent: 0, files: sized.reduce((s, x) => s + x.files, 0), count: sized.length, selected: actionable && first.selected, actionable,
      hard: first.reason === "system" || first.reason === "programs",
      overridable: !actionable && first.reason !== "system" && first.reason !== "programs" && first.reason !== "explained",
      location: sized.length === 1 ? path(units[sized[0].i].entry).slice(3) : null, elsewhere: false,
      items: sized.slice(0, 20).map((x) => ({ id: units[x.i].entry, path: path(units[x.i].entry).slice(3), bytes: x.bytes, files: x.files, blocked: null })),
    };
  }).filter((g) => g.bytes > 0);
  groups.sort((a, b) => VERDICT_ORDER.indexOf(a.verdict) - VERDICT_ORDER.indexOf(b.verdict) || b.bytes - a.bytes);
  return groups;
}

const adviceHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  advice_summary: () => {
    const groups = mockGroups();
    const totals = verdictTotals(0);
    return {
      computing: false, seq: 1, root: "C:\\",
      suggestions: { totals, reclaim: adviceNode(0).reclaim, groups, enriched: true },
      elsewhere: [],
    };
  },
  advice_detail: ({ id }) => {
    if (!units.length) buildAdvice();
    const u = unitOf[id] >= 0 ? units[unitOf[id]] : null;
    const inside = new Set(descendants(id));
    const parts = units
      .filter((x) => x !== u && inside.has(x.entry) && !entries[x.entry].removed)
      .map((x) => ({ id: x.entry, path: path(x.entry).slice(3), verdict: x.verdict, reason: x.reason, source: x.source, app: x.app, title: x.title, blocked: x.blocked, bytes: verdictTotals(x.entry)[x.verdict] }))
      .filter((p) => p.bytes > 0)
      .sort((a, b) => b.bytes - a.bytes)
      .slice(0, 8);
    const node = adviceNode(id);
    const overrideBlock = id === 0 ? "root" : node.hard ? (u && u.entry === id ? "hard" : "inside-hard") : u?.reason === "explained" ? "volume-root" : null;
    return { id, node, unit: u ? { ...u, hard: node.hard } : null, group: null, unitPath: u && u.entry !== id ? path(u.entry).slice(3) : null, parts, enriched: true, overrideBlock };
  },
  advice_refresh: () => {
    buildAdvice();
  },
  advice_cancel: () => undefined,
  advice_override: ({ ids }) => {
    const before = rolled[0].size;
    for (const id of ids as number[]) {
      for (const d of descendants(id)) entries[d].removed = true;
      entries[id].removed = true;
    }
    rollUp();
    buildAdvice();
    return { operation: { freed: before - rolled[0].size, done: (ids as number[]).length, partial: 0, skipped: 0, failed: 0, cancelled: false }, recycled: [], skipped: [] };
  },
  advice_clean: ({ keys }) => {
    const groups = mockGroups();
    const ids: number[] = [];
    for (const k of keys as string[]) {
      const m = /^(entry|within)\|(\d+)$/.exec(k);
      if (m) ids.push(Number(m[2]));
      else for (const g of groups.filter((x) => x.key === k)) ids.push(...g.items.map((i) => i.id));
    }
    const before = rolled[0].size;
    for (const id of ids) {
      for (const d of descendants(id)) entries[d].removed = true;
      entries[id].removed = true;
    }
    rollUp();
    buildAdvice();
    const freed = before - rolled[0].size;
    return { operation: { freed, done: ids.length, partial: 0, skipped: 0, failed: 0, cancelled: false }, recycled: [], skipped: [] };
  },
};

const handlers: Record<string, (a: Record<string, any>) => unknown> = {
  ...adviceHandlers,
  ...cleanHandlers,
  ...browsersHandlers,
  ...devHandlers,
  ...dupeHandlers,
  ...healthHandlers,
  app_info: () => ({ version: "0.1.0-mock", os: "browser", accent: "#0078d4" }),
  list_volumes: () => [
    { root: "C:\\", label: "Windows", fs: "NTFS", kind: "fixed", total: 4e12, available: 1.45e12, free: 1.45e12, ready: true, system: true },
    { root: "D:\\", label: "Data", fs: "NTFS", kind: "fixed", total: 2e12, available: 1.2e12, free: 1.2e12, ready: true, system: false },
    { root: "E:\\", label: "USB", fs: "exFAT", kind: "removable", total: 64e9, available: 50e9, free: 50e9, ready: true, system: false },
    { root: "F:\\", label: "", fs: "", kind: "optical", total: 0, available: 0, free: 0, ready: false, system: false },
  ],
  start_scan: () => {
    clearInterval(scanTimer);
    if (!entries.length) build();
    const started = performance.now();
    let tick = 0;
    scanTimer = setInterval(() => {
      tick++;
      const f = Math.min(1, tick / 12);
      emit("scan-progress", { scan: 1, files: Math.floor(70_000 * f), dirs: Math.floor(9_000 * f), onDisk: Math.floor(rolled[0].size * f), errors: Math.floor(3 * f), elapsedMs: performance.now() - started });
      if (f >= 1) {
        clearInterval(scanTimer);
        if (!historyCleared) historyIds = [...historyIds, historyIds[historyIds.length - 1] + 1].slice(-30);
        emit("scan-done", { scan: 1, ok: true, cancelled: false, error: null, snapshot: historyCleared ? null : historyIds[historyIds.length - 1], historyError: null });
      }
    }, 120);
    return 1;
  },
  cancel_scan: () => {
    clearInterval(scanTimer);
    emit("scan-done", { scan: 1, ok: false, cancelled: true, error: "scan cancelled", snapshot: null, historyError: null });
  },
  summary: () => ({
    root: "C:\\", rootId: 0,
    stats: { files: rolled[0].files, dirs: entries.filter((e) => e.kind === "dir").length, symlinks: 0, junctions: 1, mount_points: 0, other: 0, hardlink_dups: 0, logical: rolled[0].size, on_disk: rolled[0].size, errors: 3, approximate: 0 },
    elapsedMs: 1480, scannedAt: Math.floor(Date.now() / 1000), issues: 3, mode: "standard", volumeTotal: 4e12, volumeFree: 1.45e12,
  }),
  issues: () => [
    { path: "C:\\System Volume Information", kind: "access_denied", message: "Access is denied. (os error 5)" },
    { path: "C:\\Windows\\CSC", kind: "access_denied", message: "Access is denied. (os error 5)" },
    { path: "C:\\Users\\Default User", kind: "access_denied", message: "Access is denied. (os error 5)" },
  ],
  children: ({ id }) => sortedKids(id).map(node),
  detail: ({ id }) => {
    const ancestors = [];
    let cur = id;
    while (cur >= 0) {
      ancestors.unshift({ id: cur, name: entries[cur].name });
      cur = entries[cur].parent;
    }
    return { ...located(id), ancestors, share: rolled[id].size / rolled[0].size };
  },
  slice: ({ id, maxDepth, minBytes, maxNodes }) => slice(id, maxDepth, minBytes, maxNodes),
  top_files: ({ id, limit }) =>
    descendants(id).filter((i) => entries[i].kind === "file").sort((a, b) => rolled[b].size - rolled[a].size).slice(0, limit).map(located),
  extensions: ({ id, limit }) => {
    const m = new Map<string, { ext: string; files: number; onDisk: number; logical: number }>();
    for (const i of descendants(id)) {
      const e = entries[i];
      if (e.kind !== "file") continue;
      const dot = e.name.lastIndexOf(".");
      const ext = dot > 0 ? e.name.slice(dot + 1) : "";
      const r = m.get(ext) ?? { ext, files: 0, onDisk: 0, logical: 0 };
      r.files++;
      r.onDisk += e.size;
      r.logical += e.size;
      m.set(ext, r);
    }
    return [...m.values()].sort((a, b) => b.onDisk - a.onDisk).slice(0, limit);
  },
  search: ({ id, filter, limit }) => {
    const hits = descendants(id).filter((i) => matches(i, filter));
    hits.sort((a, b) => rolled[b].size - rolled[a].size);
    return { rows: hits.slice(0, limit).map(located), matched: hits.length, matchedOnDisk: hits.reduce((s, i) => s + rolled[i].size, 0) };
  },
  old_and_large: ({ id, options }) => {
    const o = options as import("./types").OldLargeOptions;
    const files = descendants(id).filter((i) => entries[i].kind === "file");
    const ext = (i: number) => {
      const dot = entries[i].name.lastIndexOf(".");
      return dot > 0 ? entries[i].name.slice(dot + 1).toLowerCase() : "";
    };
    const large = files.filter((i) => rolled[i].size >= o.large_min);
    const stale = files.filter((i) => rolled[i].size >= o.stale_min && entries[i].mtime < o.stale_before);
    const inst = files.filter(
      (i) => path(i).toLowerCase().includes("\\downloads\\") && INSTALLERS.includes(ext(i)) && entries[i].mtime < o.installer_before,
    );
    const all = new Set([...large, ...stale, ...inst]);
    return {
      large: ranked(large, o.limit),
      stale: ranked(stale, o.limit),
      installers: ranked(inst, o.limit),
      distinctFiles: all.size,
      distinctOnDisk: [...all].reduce((s, i) => s + rolled[i].size, 0),
    };
  },
  empty_folders: ({ id, limit }) => {
    const isEmpty = emptyTrees();
    const empty = descendants(id).filter(isEmpty);
    const outer = empty.filter((i) => entries[i].parent === id || !isEmpty(entries[i].parent));
    outer.sort((a, b) => path(a).localeCompare(path(b)));
    return {
      rows: outer.slice(0, limit).map((i) => ({ ...located(i), nested: descendants(i).length })),
      matched: outer.length,
      totalDirs: empty.length,
    };
  },
  broken_shortcuts: ({ id, limit }) => {
    const lnks = descendants(id).filter((i) => entries[i].name.toLowerCase().endsWith(".lnk"));
    const broken = lnks
      .filter((i) => (MOCK_TARGETS[entries[i].name]?.[1] ?? "ok") !== "ok")
      .map((i) => ({ ...located(i), target: MOCK_TARGETS[entries[i].name][0], status: MOCK_TARGETS[entries[i].name][1] }))
      .sort((a, b) => Number(a.status !== "missing") - Number(b.status !== "missing") || a.id - b.id);
    return { rows: broken.slice(0, limit), checked: lnks.length, unchecked: 0, unreadable: 0 };
  },
  save_html_report: () => undefined,
  age_matrix: ({ id, limit }) => {
    const now = Math.floor(Date.now() / 1000);
    const BOUNDS = [30, 182, 365, 730, 1826];
    const col = (m: number) => {
      const d = (now - m) / 86_400;
      const i = BOUNDS.findIndex((b) => d < b);
      return i < 0 ? 5 : i;
    };
    const rowFor = (kid: number) => {
      const bytes = Array(7).fill(0);
      const files = Array(7).fill(0);
      const items = entries[kid].kind === "file" ? [kid] : descendants(kid);
      for (const i of items) {
        if (entries[i].kind !== "file" || entries[i].removed) continue;
        bytes[col(entries[i].mtime)] += entries[i].size;
        files[col(entries[i].mtime)]++;
      }
      return { id: kid, name: entries[kid].name, kind: entries[kid].kind, direct: false, total: bytes.reduce((a, b) => a + b, 0), bytes, files };
    };
    const rows = sortedKids(id).filter((k) => entries[k].kind === "dir").map(rowFor).filter((r) => r.total > 0);
    const direct = sortedKids(id).filter((k) => entries[k].kind === "file").map(rowFor);
    if (direct.length) {
      const d = direct.reduce((acc, r) => ({ ...acc, bytes: acc.bytes.map((b: number, i: number) => b + r.bytes[i]), files: acc.files.map((b: number, i: number) => b + r.files[i]), total: acc.total + r.total }));
      rows.push({ ...d, id, name: entries[id].name, kind: "dir", direct: true });
    }
    rows.sort((a, b) => b.total - a.total);
    const shown = rows.slice(0, limit);
    const other = Array(7).fill(0);
    for (const r of rows.slice(limit)) r.bytes.forEach((b: number, i: number) => (other[i] += b));
    const totals = Array(7).fill(0);
    const files = Array(7).fill(0);
    for (const r of rows) r.bytes.forEach((b: number, i: number) => ((totals[i] += b), (files[i] += r.files[i])));
    return { bucketDays: BOUNDS, rows: shown, other, totals, files };
  },
  take_startup_path: () => null,
  shell_menu_status: () => shellMenu,
  update_check: () => ({ current: "0.1.0-mock", available: null, notes: null, date: null, kind: "installer", releases: "https://github.com/congluuit/NANOFreeDisk/releases/latest" }),
  update_install: () => undefined,
  update_open_releases: () => undefined,
  shell_menu_set: ({ enabled }) => (shellMenu = Boolean(enabled)),
  owner: ({ id }) => (path(id).toLowerCase().startsWith("c:\\windows") ? "NT SERVICE\\TrustedInstaller" : path(id).toLowerCase().startsWith("c:\\users\\nano") ? "NANO-PC\\nano" : "BUILTIN\\Administrators"),
  broken_links: ({ id, limit }) => {
    const links = descendants(id).filter((i) => entries[i].kind === "junction");
    const rows = links.slice(0, limit).map((i) => ({ ...located(i), target: "C:\\Users\\nano\\AppData-old", status: "missing" }));
    return { rows, checked: links.length, unchecked: 0, unreadable: 0 };
  },
  explain: ({ id }) => {
    const p = path(id);
    const hit = MOCK_ARTICLES.find((a) => p.toLowerCase() === a.path.toLowerCase() || p.toLowerCase().startsWith(a.path.toLowerCase() + "\\"));
    return hit ? { ...hit, inside: p.toLowerCase() !== hit.path.toLowerCase(), rules: [] } : null;
  },
  history_snapshots: ({ root }) =>
    historyCleared || String(root).toLowerCase() !== "c:\\" ? [] : [...historyIds].reverse().map(snapshotRow),
  history_diff: ({ older, newer }) => mockDiff(older, newer),
  history_info: () => ({ snapshots: historyCleared ? 0 : historyIds.length, path: "C:\\Users\\nano\\AppData\\Local\\NANOFreeDisk\\history.db" }),
  history_clear: () => {
    historyCleared = true;
  },
  find_path: ({ path: p }) => {
    let cur = 0;
    for (const part of String(p).split(/[\\/]/).filter(Boolean)) {
      const next = kids[cur].find((k) => !entries[k].removed && entries[k].name.toLowerCase() === part.toLowerCase());
      if (next === undefined) return null;
      cur = next;
    }
    return cur;
  },
  health: () => {
    const findings = [
      { kind: "old-large", severity: "warn", penalty: 10, subject: "C:\\", value: 51, bytes: 2.0e12, fix: "oldlarge" },
      { kind: "filling-fast", severity: "warn", penalty: 7, subject: "C:\\", value: 45, bytes: 1.45e12, fix: "analyze" },
      { kind: "low-space", severity: "warn", penalty: 5, subject: "E:\\", value: 18, bytes: 1.1e10, fix: "oldlarge" },
    ];
    if (!entries.length) findings.unshift({ kind: "no-scan", severity: "info", penalty: 0, subject: "", value: 0, bytes: 0, fix: "home" });
    const score = 100 - findings.reduce((s, f) => s + f.penalty, 0);
    return { score, grade: score >= 85 ? "good" : score >= 60 ? "fair" : "poor", findings };
  },
  reveal: () => undefined,
  open_item: () => undefined,
  plan_delete: ({ ids }) => {
    const items = [...new Set<number>(ids)].filter((i) => i > 0).map(check);
    const ok = items.filter((i) => i.allowed);
    const onDisk = ok.reduce((s, i) => s + i.onDisk, 0);
    return { items, onDisk, files: ok.reduce((s, i) => s + i.files, 0), extraConfirm: onDisk > 50 * 2 ** 30 || onDisk > 4e12 / 5 };
  },
  recycle: ({ ids }) => {
    const out = [...new Set<number>(ids)].map((id) => {
      const c = check(id);
      if (c.allowed && !entries[id].removed) {
        for (const d of descendants(id)) entries[d].removed = true;
        entries[id].removed = true;
      }
      return { id, status: c.allowed ? "done" : "blocked", reason: c.reason, message: null, onDisk: c.onDisk };
    });
    rollUp();
    buildAdvice();
    return out;
  },
  delete_entries: ({ ids, mode, kind }) => {
    const items = [...new Set<number>(ids)].map(check).filter((c) => c.allowed && !entries[c.id].removed);
    const out = handlers.recycle({ ids }) as { status: string }[];
    mockDeleted(kind ?? "manual", items.map((c) => ({ path: c.path, bytes: c.onDisk, isDir: c.kind === "dir" })), mode ?? "recycle-bin");
    return out;
  },
  export_report: () => undefined,
  load_settings: () => {
    try {
      return JSON.parse(localStorage.getItem("nfd-settings") ?? "null");
    } catch {
      return null;
    }
  },
  save_settings: ({ settings }) => {
    try {
      localStorage.setItem("nfd-settings", JSON.stringify(settings));
    } catch {
      /* private mode */
    }
  },
  read_text_file: () => "",
  write_text_file: () => undefined,
};

export async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const h = handlers[cmd] ?? hwHandlers[cmd] ?? monitorHandlers[cmd];
  if (!h) throw { code: "unknown", message: `mock: no command ${cmd}` };
  if (!entries.length && !(cmd in hwHandlers) && !(cmd in monitorHandlers) && cmd !== "list_volumes" && cmd !== "app_info" && cmd !== "load_settings") build();
  await new Promise((r) => setTimeout(r, 0));
  return h(args) as T;
}

export async function listen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  const set = listeners.get(event) ?? new Set();
  set.add(cb as (p: unknown) => void);
  listeners.set(event, set);
  return () => set.delete(cb as (p: unknown) => void);
}
