#!/usr/bin/env node
// End-to-end smoke test of the desktop app on Windows: starts the built exe with
// WebView2 remote debugging, drives the real IPC commands through the page, checks
// the results, saves screenshots and closes the app.
//
//   node scripts/gui-smoke.mjs [path\to\nanofreedisk.exe] [--shots dir]
//
// What it covers: app_info, list_volumes, a scan of a temporary fixture (children,
// detail, slice, search, extensions), Old & Large, empty folders, broken shortcuts,
// the HTML report writer, the delete plan refusing a folder inside %SystemRoot% and
// accepting a fixture file, an actual move of one fixture file to the Recycle Bin,
// the new views / Ctrl+K palette, the scan history (two snapshots of the fixture,
// "What changed?", find_path), the Health Score and the Hardware view (inventory,
// sensors, Tasks with its hard blocks). Nothing outside the fixture is ever deleted
// and the only process ever ended is a `ping` the script starts itself; the
// fixture's history snapshots are forgotten at the end.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const shotsIdx = args.indexOf("--shots");
const shots = shotsIdx >= 0 ? resolve(args[shotsIdx + 1]) : null;
const exe = resolve(args.find((a, i) => !a.startsWith("--") && i !== shotsIdx + 1) ?? join(root, "target", "release", "nanofreedisk.exe"));
const PORT = 9333 + Math.floor(Math.random() * 500);

if (process.platform !== "win32") {
  console.log("gui-smoke: Windows only, skipped");
  process.exit(0);
}
if (!existsSync(exe)) {
  console.error(`gui-smoke: ${exe} not found (build with: pnpm --dir apps/nanofreedisk tauri build --no-bundle)`);
  process.exit(2);
}

// ── fixture ──────────────────────────────────────────────────────────────────
const fixture = mkdtempSync(join(tmpdir(), "nfd-smoke-"));
mkdirSync(join(fixture, "projects", "game", "Library"), { recursive: true });
mkdirSync(join(fixture, "videos"), { recursive: true });
writeFileSync(join(fixture, "projects", "game", "Library", "cache.bin"), Buffer.alloc(3 * 1024 * 1024, 1));
writeFileSync(join(fixture, "projects", "game", "main.rs"), "fn main() {}\n");
writeFileSync(join(fixture, "videos", "clip.mp4"), Buffer.alloc(2 * 1024 * 1024, 2));
writeFileSync(join(fixture, "old-installer.iso"), Buffer.alloc(1024 * 1024, 3));
const victim = join(fixture, "old-installer.iso");
// A2: an old installer in Downloads; A3: an empty folder tree and two shortcuts
mkdirSync(join(fixture, "Downloads"));
writeFileSync(join(fixture, "Downloads", "old-setup.msi"), Buffer.alloc(1024, 4));
const yearAgo = new Date(Date.now() - 400 * 86_400_000);
utimesSync(join(fixture, "Downloads", "old-setup.msi"), yearAgo, yearAgo);
mkdirSync(join(fixture, "empty", "inner"), { recursive: true });
/** Minimal shell link whose LinkInfo points at `target` (ASCII path). */
function lnk(target) {
  const head = Buffer.alloc(0x4c);
  head.writeUInt32LE(0x4c, 0);
  Buffer.from("0114020000000000c000000000000046", "hex").copy(head, 4);
  head.writeUInt32LE(2, 0x14); // HasLinkInfo
  const path = Buffer.from(`${target}\0\0`, "latin1");
  const info = Buffer.alloc(0x1c);
  [0x1c + path.length, 0x1c, 1, 0, 0x1c, 0, 0x1c + path.length - 1].forEach((v, i) => info.writeUInt32LE(v, i * 4));
  return Buffer.concat([head, info, path, Buffer.alloc(4)]);
}
writeFileSync(join(fixture, "ok.lnk"), lnk(join(fixture, "videos", "clip.mp4")));
writeFileSync(join(fixture, "broken.lnk"), lnk(join(fixture, "gone.exe")));
const FILES = 7;

let pinger = null;
const failures = [];
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures.push(name);
};

// ── launch ───────────────────────────────────────────────────────────────────
const child = spawn(exe, [], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  stdio: "ignore",
});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function target() {
  for (let i = 0; i < 80; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
      const page = list.find((t) => t.type === "page");
      if (page) return page;
    } catch {
      /* not up yet */
    }
    await sleep(250);
  }
  throw new Error("WebView2 debugging endpoint did not come up");
}

let ws;
let seq = 0;
const pending = new Map();
function send(method, params = {}) {
  const id = ++seq;
  ws.send(JSON.stringify({ id, method, params }));
  return new Promise((res, rej) => pending.set(id, { res, rej }));
}
async function evaluate(expression) {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
  return r.result.value;
}
const invoke = (cmd, a = {}) =>
  evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(a)}).then(v => v, e => ({ __error: e }))`);

async function shot(name) {
  if (!shots) return;
  mkdirSync(shots, { recursive: true });
  const { data } = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(shots, `${name}.png`), Buffer.from(data, "base64"));
}

async function scan(path, record = true) {
  const id = await invoke("start_scan", { path, record });
  for (let i = 0; i < 200; i++) {
    await sleep(100);
    const s = await invoke("summary");
    if (!s.__error && s.root.toLowerCase() === path.toLowerCase().replace(/\\$/, "")) return s;
  }
  throw new Error(`scan ${id} of ${path} did not finish`);
}

try {
  const page = await target();
  ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r, j) => {
    ws.onopen = r;
    ws.onerror = j;
  });
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    const p = pending.get(msg.id);
    if (p) {
      pending.delete(msg.id);
      msg.error ? p.rej(new Error(msg.error.message)) : p.res(msg.result);
    }
  };
  for (let i = 0; i < 40 && !(await evaluate("!!window.__TAURI_INTERNALS__ && !!document.querySelector('.side')")); i++) await sleep(250);
  await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
  await sleep(800);
  await shot("1-home");

  const info = await invoke("app_info");
  check("app_info", typeof info.version === "string", `v${info.version}, accent ${info.accent}`);
  const vols = await invoke("list_volumes");
  check("list_volumes has the system drive", vols.some((v) => v.system && v.total > 0), vols.map((v) => v.root).join(" "));

  const s = await scan(fixture);
  check("scan totals", s.stats.files === FILES && s.stats.on_disk >= 6 * 1024 * 1024, `${s.stats.files} files, ${s.stats.on_disk} B`);
  const kids = await invoke("children", { id: 0 });
  check("children sorted by size", kids[0]?.name === "projects", kids.map((k) => k.name).join(", "));
  const slice = await invoke("slice", { id: 0, maxDepth: 8, minBytes: 0, maxNodes: 1000 });
  check("slice", slice.ids.length >= 8 && slice.parents[0] === -1, `${slice.ids.length} nodes`);
  const found = await invoke("search", { id: 0, filter: { name: "*.iso" }, limit: 10 });
  check("search *.iso", found.matched === 1 && found.rows[0].path === victim);
  const ext = await invoke("extensions", { id: 0, limit: 10 });
  check("extensions", ext[0]?.ext === "bin", ext.map((e) => e.ext).join(","));

  // A2 Old & Large
  const now = Math.floor(Date.now() / 1000);
  const ol = await invoke("old_and_large", {
    id: 0,
    options: { large_min: 2 * 1024 * 1024, stale_min: 1, stale_before: now - 365 * 86_400, installer_before: now - 30 * 86_400, limit: 10 },
  });
  check("old & large: large files", ol.large?.rows.map((r) => r.name).join(",") === "cache.bin,clip.mp4", JSON.stringify(ol.large?.rows.map((r) => r.name)));
  check("old & large: old installer in Downloads", ol.installers?.matched === 1 && ol.installers.rows[0].name === "old-setup.msi");
  check("old & large: stale", ol.stale?.rows.some((r) => r.name === "old-setup.msi"));
  // A3 empty folders and broken shortcuts (read-only)
  const empty = await invoke("empty_folders", { id: 0, limit: 50 });
  check("empty folders", empty.rows?.length === 1 && empty.rows[0].name === "empty" && empty.rows[0].nested === 1, JSON.stringify(empty.rows?.map((r) => r.path)));
  const links = await invoke("broken_shortcuts", { id: 0, limit: 50 });
  check(
    "broken shortcuts",
    links.checked === 2 && links.rows?.length === 1 && links.rows[0].name === "broken.lnk" && links.rows[0].status === "missing",
    JSON.stringify(links),
  );
  // A6 report writer: only *.html files
  const reportPath = join(fixture, "report.html");
  await invoke("save_html_report", { path: reportPath, html: "<!doctype html><p>ok</p>" });
  check("save_html_report writes the file", existsSync(reportPath) && readFileSync(reportPath, "utf8").includes("<p>ok</p>"));
  const refused = await invoke("save_html_report", { path: join(fixture, "evil.exe"), html: "x" });
  check("save_html_report refuses other extensions", refused.__error?.code === "bad-path" && !existsSync(join(fixture, "evil.exe")));
  rmSync(reportPath);

  // UI reflects the scan
  await evaluate("document.querySelectorAll('.side .nav')[1].click()");
  await sleep(1200);
  await shot("2-analyze-fixture");

  // delete plan: fixture file allowed, nothing else touched yet
  const isoId = found.rows[0].id;
  const plan = await invoke("plan_delete", { ids: [isoId] });
  check("plan allows a fixture file", plan.items[0]?.allowed === true);
  const out = await invoke("recycle", { ids: [isoId] });
  check("recycle moved the file", out[0]?.status === "done" && !existsSync(victim), JSON.stringify(out[0]));
  const after = await invoke("summary");
  check("totals updated after recycle", after.stats.files === FILES - 1);

  // protected locations are refused (plan only — nothing is deleted)
  const sysHelp = join(process.env.SystemRoot ?? "C:\\Windows", "Help");
  if (existsSync(sysHelp)) {
    await scan(sysHelp, false);
    const sysKids = await invoke("children", { id: 0 });
    if (sysKids.length) {
      const p = await invoke("plan_delete", { ids: sysKids.map((k) => k.id) });
      check("plan refuses %SystemRoot% content", p.items.every((i) => !i.allowed && i.reason === "system-dir"), `${p.items.length} items`);
    }
  }
  // UI screenshots in both languages / themes; the fixture grows by 12 MB first so
  // the second snapshot has something to report
  writeFileSync(join(fixture, "videos", "new-clip.mp4"), Buffer.alloc(12 * 1024 * 1024, 5));
  await scan(fixture);
  const snaps = await invoke("history_snapshots", { root: fixture });
  check("history keeps both fixture scans", snaps.length === 2 && snaps[0].startedAt >= snaps[1].startedAt, JSON.stringify(snaps.map?.((x) => x.id) ?? snaps));
  if (snaps.length === 2) {
    const diff = await invoke("history_diff", { older: snaps[1].id, newer: snaps[0].id, minDelta: 10 * 1024 * 1024, limit: 50 });
    check(
      "what changed: videos grew by the new clip",
      diff.grew?.[0]?.path === "videos" && diff.grew[0].delta >= 12 * 1024 * 1024,
      JSON.stringify(diff.grew),
    );
    check("what changed: the new large file", diff.filesUp?.some((f) => f.path.endsWith("new-clip.mp4") && f.before === 0), JSON.stringify(diff.filesUp));
  }
  const videos = (await invoke("children", { id: 0 })).find((k) => k.name === "videos");
  check("find_path resolves a relative path", (await invoke("find_path", { path: "VIDEOS" })) === videos?.id);
  const healthStart = Date.now();
  const health = await invoke("health");
  const healthMs = Date.now() - healthStart;
  check("health score", Number.isInteger(health.score) && health.score >= 0 && health.score <= 100 && Array.isArray(health.findings), `${health.score} ${health.grade} ${health.findings?.map((f) => f.kind).join(",")} in ${healthMs} ms`);
  for (let i = 0; i < 40 && !(await evaluate("!!document.querySelector('.tree .row')")); i++) await sleep(150);
  check("UI shows the scan", await evaluate("!!document.querySelector('.tree .row')"));
  await sleep(500);
  await evaluate(`(() => { const row = [...document.querySelectorAll('.tree .row')][0]; row.dispatchEvent(new MouseEvent('click', {bubbles: true})); document.querySelector('.tree').dispatchEvent(new KeyboardEvent('keydown', {key: 'Delete', bubbles: true})); })()`);
  await sleep(800);
  await shot("3-delete-dialog");
  await evaluate("document.querySelector('.dialog .cancel')?.click()");
  await sleep(300);

  // new views and the command palette
  const nav = (i) => evaluate(`document.querySelectorAll('.side .nav')[${i}].click()`);
  await nav(2);
  await sleep(800);
  await evaluate("document.querySelectorAll('.tabs .tab')[2].click()");
  await sleep(500);
  check("Old & large view lists the installer", await evaluate("document.querySelector('main').innerText.includes('old-setup.msi')"));
  await shot("4-old-large");
  await nav(3);
  for (let i = 0; i < 30 && !(await evaluate("document.querySelector('main').innerText.includes('broken.lnk')")); i++) await sleep(150);
  check("Tidy view lists the broken shortcut", await evaluate("document.querySelector('main').innerText.includes('broken.lnk')"));
  await shot("5-tidy");
  await evaluate("window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))");
  await sleep(300);
  await evaluate(`(() => { const i = document.querySelector('.palette input'); i.value = 'proj'; i.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  await sleep(600);
  check("Ctrl+K palette finds a folder of the scan", await evaluate("[...document.querySelectorAll('.palette .item')].some((x) => x.innerText.includes('projects'))"));
  await shot("6-palette");
  await evaluate("document.querySelector('.palette input').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))");
  await sleep(200);
  check("palette closes on Esc", await evaluate("!document.querySelector('.palette')"));

  // What changed? tab and the Health Score on Home
  await nav(1);
  await sleep(800);
  await evaluate("document.querySelector('[data-tab=changes]')?.click()");
  for (let i = 0; i < 30 && !(await evaluate("document.querySelector('.changes')?.innerText.includes('new-clip.mp4') ?? false")); i++) await sleep(150);
  check("What changed? tab lists the new clip", await evaluate("document.querySelector('.changes')?.innerText.includes('new-clip.mp4') ?? false"));
  await shot("7-changes");
  await nav(0);
  for (let i = 0; i < 100 && !(await evaluate("!!document.querySelector('.health .num b')")); i++) await sleep(100);
  check("Home shows the health score", await evaluate("!!document.querySelector('.health .num b')"));
  for (let i = 0; i < 50 && !(await evaluate("!!document.querySelector('.temps [data-temp=gpu]')")); i++) await sleep(100);
  check("Home shows the temperature card", await evaluate("document.querySelectorAll('.temps [data-temp]').length === 3"));
  await shot("8-home-health");

  // Hardware view (docs/HARDWARE_MONITOR.md): Overview, Sensors, Tasks. Tasks only
  // ever ends a child process this script starts; system processes are checked only.
  const inv = await invoke("hw_inventory");
  check(
    "hw inventory",
    inv.schema === "nanofreedisk.hwinfo.v1" && inv.cpu.threads > 0 && inv.memory.total > 0,
    `${inv.cpu.name}, ${inv.cpu.cores}C/${inv.cpu.threads}T, ${inv.gpus?.length} GPU, ${inv.disks?.length} disk(s)`,
  );
  check("hw inventory carries no serial numbers", !/serial/i.test(JSON.stringify(inv)));
  const hwNav = await evaluate("[...document.querySelectorAll('.side .nav')].findIndex((b) => /Hardware|Phần cứng/.test(b.innerText))");
  check("Hardware is in the sidebar", hwNav >= 0);
  await nav(hwNav);
  await evaluate("document.querySelector('[data-hwtab=overview]')?.click()");
  for (let i = 0; i < 60 && !(await evaluate("!!document.querySelector('[data-sec=cpu]')")); i++) await sleep(150);
  check("Overview shows the CPU card", await evaluate("document.querySelector('[data-sec=cpu]')?.innerText.includes(" + JSON.stringify(inv.cpu.name.split(" ").pop()) + ") ?? false"));
  await shot("9-hw-overview");

  await invoke("sensors_snapshot");
  await sleep(1100);
  const sens = await invoke("sensors_snapshot");
  const cpuTotal = sens.sensors?.find((x) => x.id === "cpu/cpu-total");
  check("sensors: CPU load", cpuTotal?.value >= 0 && cpuTotal.value <= 100, `${cpuTotal?.value?.toFixed(1)} %, ${sens.sensors?.length} sensors in ${sens.sample_ms?.toFixed(1)} ms`);
  check("sensors: memory load", sens.sensors?.some((x) => x.id === "memory/mem-load" && x.value > 0));
  check("sensors: CPU temperature explains PawnIO", sens.notes?.some((n) => n.device === "cpu" && n.reason === "needs-pawnio") || sens.sensors?.some((x) => x.device === "cpu" && x.kind === "temperature"));
  check("sensors: reset min/max", (await invoke("sensors_reset_minmax"))?.__error === undefined);
  await evaluate("document.querySelector('[data-hwtab=sensors]').click()");
  for (let i = 0; i < 40 && !(await evaluate("!!document.querySelector('[data-sensor=\"cpu/cpu-total\"]')")); i++) await sleep(150);
  check("Sensors tab shows CPU Total", await evaluate("!!document.querySelector('[data-sensor=\"cpu/cpu-total\"]')"));
  await shot("10-hw-sensors");

  pinger = spawn("ping", ["-n", "120", "127.0.0.1"], { stdio: "ignore" });
  const alive = (pid) => {
    try {
      process.kill(pid, 0);
      return true;
    } catch {
      return false;
    }
  };
  await invoke("tasks_snapshot");
  await sleep(600);
  const tasks = await invoke("tasks_snapshot");
  const me = tasks.tasks?.find((x) => x.pid === pinger.pid);
  check("tasks: list has the smoke's child", !!me && tasks.tasks.length > 20, `${tasks.tasks?.length} processes in ${tasks.elapsed_ms?.toFixed(1)} ms`);
  if (me) {
    const v = await invoke("task_verdict", { pid: me.pid, createTime: me.create_time });
    check("tasks: ping.exe needs the second confirmation", v.verdict === "warn" && v.reasons.includes("windows-component"), JSON.stringify(v));
    const stale = await invoke("task_end", { pid: me.pid, createTime: "116444736000000000", mode: "kill", confirmed: true });
    check("tasks: wrong creation time ends nothing", stale[0]?.outcome?.status === "changed" && alive(me.pid), JSON.stringify(stale));
    const once = await invoke("task_end", { pid: me.pid, createTime: me.create_time, mode: "kill", confirmed: false });
    check("tasks: one confirmation is not enough", once[0]?.outcome?.status === "needs-confirm" && alive(me.pid), JSON.stringify(once));
  }
  for (const name of ["csrss.exe", "wininit.exe", "lsass.exe"]) {
    const sys = tasks.tasks?.find((x) => x.name.toLowerCase() === name);
    if (!sys) continue;
    const v = await invoke("task_verdict", { pid: sys.pid, createTime: sys.create_time });
    const r = await invoke("task_end", { pid: sys.pid, createTime: sys.create_time, mode: "kill", confirmed: true });
    check(`tasks: ${name} is hard-blocked`, sys.blocked && v.verdict === "blocked" && r[0]?.outcome?.status === "blocked", JSON.stringify(r));
  }
  const self = tasks.tasks?.find((x) => x.pid === child.pid);
  if (self) {
    const r = await invoke("task_end", { pid: self.pid, createTime: self.create_time, mode: "kill", confirmed: true });
    check("tasks: the app cannot end itself", r[0]?.outcome?.detail === "own", JSON.stringify(r));
  }
  await evaluate("document.querySelector('[data-hwtab=tasks]').click()");
  for (let i = 0; i < 40 && !(await evaluate(`!!document.querySelector('[data-pid="${pinger.pid}"]')`)); i++) await sleep(150);
  check("Tasks tab lists the child", await evaluate(`!!document.querySelector('[data-pid="${pinger.pid}"]')`));
  await shot("11-hw-tasks");
  // the real UI path: right-click → End process → confirm twice
  await evaluate(`document.querySelector('[data-pid="${pinger.pid}"]').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 300, clientY: 300 }))`);
  await sleep(300);
  await evaluate("[...document.querySelectorAll('.menu button')].find((b) => b.classList.contains('danger'))?.click()");
  for (let i = 0; i < 30 && !(await evaluate("!!document.querySelector('.dialog .extra input')")); i++) await sleep(100);
  await shot("12-hw-end-dialog");
  check("End dialog keeps the button disabled until acknowledged", await evaluate("document.querySelector('.dialog footer .danger')?.disabled === true"));
  await evaluate("document.querySelector('.dialog .extra input').click()");
  await sleep(150);
  await evaluate("document.querySelector('.dialog footer .danger').click()");
  for (let i = 0; i < 40 && alive(pinger.pid); i++) await sleep(100);
  check("End process from the dialog ends the child", !alive(pinger.pid));
} catch (e) {
  check("smoke run", false, e.message);
} finally {
  try {
    const removed = await invoke("history_forget", { root: fixture });
    if (removed !== 2) console.log(`note: forgot ${JSON.stringify(removed)} fixture snapshot(s)`);
  } catch {
    /* app already gone */
  }
  try {
    ws?.close();
  } catch {
    /* ignore */
  }
  child.kill();
  pinger?.kill();
  await sleep(500);
  rmSync(fixture, { recursive: true, force: true });
}

if (failures.length) {
  console.error(`gui-smoke: ${failures.length} failure(s)`);
  process.exit(1);
}
console.log("gui-smoke: all checks passed");
