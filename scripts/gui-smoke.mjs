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
// "What changed?", find_path) and the Health Score; Phase 3: the Activity log and
// Restore of the Recycle Bin move, Tidy up moving the empty folder to the quarantine
// and back, and Clean with a rule pack that only matches the fixture (scan, clean,
// restore, the Clean / Activity views); the Hardware view (inventory, sensors, Tasks
// with its hard blocks); Phase 4: the DevCache view with a devcache pack that only
// matches its own fixture (clean + restore, the 🔒 Keep list), the Repo tab's build
// output (quarantine + restore), toolchains, package stores (listed, never run — a
// command line that was not confirmed is refused), unknown folders and the guarded
// reveal / propose commands, AI leftovers on their own fixture (sessions, kept chat
// history, a git worktree removed by git, quarantine + restore); Phase 5: duplicates in their own fixture (groups, keeper,
// refusal of a selection that keeps nothing, quarantine + restore, a keeper changed
// after the search, preview / reveal / export guards, the view and its confirmation,
// the Explorer verb); Phase 6: the disk activity monitor with I/O counters (this
// script's own writes attributed to it, chart, disks, pause, the reveal guard, the
// view's tabs — file detail is not turned on: it would ask UAC, and nothing is
// recorded into the history); Phase 7: the health report (drives, volumes, the JSON
// export without serial numbers and its *.json guard), the Health view and the score.
// Nothing outside the fixture is ever deleted and the only process ever ended is a
// `ping` the script starts itself; the fixture's history snapshots and journal
// entries are forgotten at the end.

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
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
// A5: a node_modules folder the encyclopedia knows; A3: an orphan junction
mkdirSync(join(fixture, "projects", "web", "node_modules", "pkg"), { recursive: true });
writeFileSync(join(fixture, "projects", "web", "node_modules", "pkg", "index.js"), "module.exports = 1;\n");
mkdirSync(join(fixture, "junction-target"));
execFileSync("cmd", ["/C", "mklink", "/J", join(fixture, "old-link"), join(fixture, "junction-target")], { stdio: "ignore" });
rmSync(join(fixture, "junction-target"), { recursive: true });
const FILES = 8;
// Phase 3: junk for a test rule pack that matches nothing but the fixture
mkdirSync(join(fixture, "junk", "cache"), { recursive: true });
writeFileSync(join(fixture, "junk", "cache", "a.bin"), Buffer.alloc(64 * 1024, 6));
writeFileSync(join(fixture, "junk", "old.log"), "log line\n");
const JUNK_FILES = 2;
const rulesDir = mkdtempSync(join(tmpdir(), "nfd-smoke-rules-"));
writeFileSync(
  join(rulesDir, "smoke.toml"),
  `[pack]
id = "smoke"
name = "Smoke test"
version = "0.0.1"

[[rule]]
id = "smoke.junk"
vendor = "NANO"
app = "Smoke fixture"
category = "test"
paths = ["\${TEMP}/${basename(fixture)}/junk/**"]
level = "safe"
regenerates = true
impact = "Files of the GUI smoke test fixture."
impact_vi = "Tệp của fixture kiểm thử GUI."
verified_on = 2026-10-16
`,
);
// Phase 4: a DevCache pack and a project with build output, in their own fixture
const devFixture = mkdtempSync(join(tmpdir(), "nfd-smoke-dev-"));
mkdirSync(join(devFixture, "caches", "blobs"), { recursive: true });
writeFileSync(join(devFixture, "caches", "blobs", "x.bin"), Buffer.alloc(32 * 1024, 7));
writeFileSync(join(devFixture, "caches", "index.db"), "idx\n");
mkdirSync(join(devFixture, "keep"), { recursive: true });
writeFileSync(join(devFixture, "keep", "settings.json"), "{}\n");
const devProject = join(devFixture, "projects", "smoke-app");
mkdirSync(join(devProject, "target", "debug"), { recursive: true });
writeFileSync(join(devProject, "Cargo.toml"), "[package]\nname = \"smoke-app\"\n");
writeFileSync(join(devProject, "target", "debug", "smoke-app.exe"), Buffer.alloc(16 * 1024, 8));
const monthAgo = new Date(Date.now() - 30 * 86_400_000);
for (const p of [
  join(devProject, "target", "debug", "smoke-app.exe"),
  join(devProject, "target", "debug"),
  join(devProject, "target"),
  join(devProject, "Cargo.toml"),
  devProject,
]) {
  utimesSync(p, monthAgo, monthAgo);
}
const devRulesDir = mkdtempSync(join(tmpdir(), "nfd-smoke-devrules-"));
writeFileSync(
  join(devRulesDir, "smoke-dev.toml"),
  `[pack]
id = "smoke-dev"
name = "Smoke DevCache"
version = "0.0.1"
devcache = true

[[rule]]
id = "smoke.dev.cache"
vendor = "NANO Dev"
app = "Smoke Tool"
category = "test"
paths = ["\${TEMP}/${basename(devFixture)}/caches/**"]
level = "safe"
regenerates = true
impact = "Caches of the GUI smoke test fixture."
impact_vi = "Bộ đệm của fixture kiểm thử GUI."
verified_on = 2026-10-16

[[rule]]
id = "smoke.dev.keep"
vendor = "NANO Dev"
app = "Smoke Tool"
category = "test"
paths = ["\${TEMP}/${basename(devFixture)}/keep/**"]
level = "protected"
impact = "Settings of the GUI smoke test fixture."
impact_vi = "Thiết lập của fixture kiểm thử GUI."
verified_on = 2026-10-16
`,
);
// Phase 5: duplicates in their own fixture (never written to the hash cache)
const dupeFixture = mkdtempSync(join(tmpdir(), "nfd-smoke-dupes-"));
const dupeA = randomBytes(1536 * 1024);
const dupeB = randomBytes(1200 * 1024);
mkdirSync(join(dupeFixture, "Photos", "2019"), { recursive: true });
mkdirSync(join(dupeFixture, "Downloads"), { recursive: true });
mkdirSync(join(dupeFixture, "node_modules", "pkg"), { recursive: true });
writeFileSync(join(dupeFixture, "Photos", "2019", "video.mp4"), dupeA);
writeFileSync(join(dupeFixture, "Downloads", "video.mp4"), dupeA);
writeFileSync(join(dupeFixture, "Downloads", "video (1).mp4"), dupeA);
writeFileSync(join(dupeFixture, "Photos", "pic.png"), dupeB);
writeFileSync(join(dupeFixture, "Downloads", "pic.png"), dupeB);
writeFileSync(join(dupeFixture, "node_modules", "pkg", "video.mp4"), dupeA);
writeFileSync(join(dupeFixture, "small-a.txt"), "same small text\n");
writeFileSync(join(dupeFixture, "small-b.txt"), "same small text\n");
for (const p of ["Photos/2019/video.mp4", "Photos/pic.png"]) utimesSync(join(dupeFixture, p), yearAgo, yearAgo);
// AI leftovers: a fake home with Claude Code sessions, a project with agent output
// (a git worktree when git is installed), %TEMP%\claude and a Playwright store — the
// scan reads only this folder (`window.__nfdAiFixture`), never the real sessions.
const aiFixture = mkdtempSync(join(tmpdir(), "nfd-smoke-ai-"));
const aiProject = join(aiFixture, "proj");
const SID_DONE = "0a0a0a0a-0000-4000-8000-00000000000a";
const SID_OPEN = "0b0b0b0b-0000-4000-8000-00000000000b";
mkdirSync(join(aiProject, ".playwright-mcp"), { recursive: true });
writeFileSync(join(aiProject, "README.md"), "smoke\n");
writeFileSync(join(aiProject, ".playwright-mcp", "console-1.log"), "log\n");
writeFileSync(join(aiProject, ".aider.chat.history.md"), "# chat\n");
for (const [sid, title] of [
  [SID_DONE, "Smoke finished session"],
  [SID_OPEN, "Smoke open session"],
]) {
  const transcript = join(aiFixture, "home", ".claude", "projects", "smoke", `${sid}.jsonl`);
  mkdirSync(dirname(transcript), { recursive: true });
  writeFileSync(
    transcript,
    `${JSON.stringify({ type: "user", cwd: aiProject, timestamp: "2020-01-01T00:00:00Z" })}\n${JSON.stringify({ type: "custom-title", customTitle: title })}\n`,
  );
  utimesSync(transcript, monthAgo, monthAgo);
  const scratch = join(aiFixture, "temp", "claude", "smoke-proj", sid, "scratchpad");
  mkdirSync(scratch, { recursive: true });
  writeFileSync(join(scratch, "build.log"), Buffer.alloc(8 * 1024, 3));
  for (const p of [join(scratch, "build.log"), scratch, dirname(scratch)]) utimesSync(p, monthAgo, monthAgo);
}
mkdirSync(join(aiFixture, "desk", "acc", "org"), { recursive: true });
writeFileSync(
  join(aiFixture, "desk", "acc", "org", `local_${SID_OPEN}.json`),
  JSON.stringify({ sessionId: `local_${SID_OPEN}`, cliSessionId: SID_OPEN, cwd: aiProject, title: "Smoke open session", lastActivityAt: monthAgo.getTime(), isArchived: false }),
);
for (const rev of ["chromium-1000", "chromium-1001", "mcp-chrome-smoke"]) {
  mkdirSync(join(aiFixture, "ms-playwright", rev), { recursive: true });
  writeFileSync(join(aiFixture, "ms-playwright", rev, "chrome.bin"), Buffer.alloc(4096, 5));
}
for (const p of [join(aiProject, ".playwright-mcp", "console-1.log"), join(aiProject, ".playwright-mcp")]) utimesSync(p, monthAgo, monthAgo);
let aiGit = false;
const aiWorktree = join(aiProject, ".claude", "worktrees", "smoke-done");
try {
  const git = (...a) => execFileSync("git", ["-c", "user.name=smoke", "-c", "user.email=smoke@example.com", "-c", "commit.gpgsign=false", "-C", aiProject, ...a], { stdio: "ignore" });
  writeFileSync(join(aiProject, ".gitignore"), ".claude/worktrees/\ntarget/\n.playwright-mcp/\n");
  git("init", "-q", "-b", "main");
  git("add", "README.md", ".gitignore");
  git("commit", "-q", "-m", "init");
  git("worktree", "add", "-q", "-b", "smoke/done", aiWorktree);
  mkdirSync(join(aiWorktree, "target"), { recursive: true });
  writeFileSync(join(aiWorktree, "target", "app.bin"), Buffer.alloc(16 * 1024, 9));
  aiGit = true;
} catch {
  /* no git: the worktree part is skipped */
}
const operations = [];

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

const nav = (i) => evaluate(`document.querySelectorAll('.side .nav')[${i}].click()`);

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
  check("scan totals", s.stats.files === FILES + JUNK_FILES && s.stats.on_disk >= 6 * 1024 * 1024, `${s.stats.files} files, ${s.stats.on_disk} B`);
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
  // Tidy up: the empty folder tree goes to the quarantine and comes back
  if (empty.rows?.length) {
    const emptyDir = join(fixture, "empty");
    const tidy = await invoke("delete_entries", { ids: [empty.rows[0].id], mode: "quarantine", kind: "tidy" });
    check("tidy: empty folder moved to the quarantine", tidy[0]?.status === "done" && !existsSync(emptyDir), JSON.stringify(tidy));
    const ops = await invoke("activity_list", { limit: 5 });
    const op = ops.find?.((o) => o.kind === "tidy");
    if (op) operations.push(op.id);
    check("activity lists the tidy operation", op?.restorable === 1, JSON.stringify(ops[0]));
    if (op) {
      const back = await invoke("activity_restore", { id: op.id });
      check("tidy: restore brings the folder back", back[0]?.status === "restored" && existsSync(join(emptyDir, "inner")), JSON.stringify(back));
    }
  }
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
  check("totals updated after recycle", after.stats.files === FILES + JUNK_FILES - 1);
  const manual = (await invoke("activity_list", { limit: 5 })).find?.((o) => o.kind === "manual");
  if (manual) operations.push(manual.id);
  check("activity lists the Recycle Bin move", manual?.done === 1 && manual?.restorable === 1, JSON.stringify(manual));
  if (manual) {
    const back = await invoke("activity_restore", { id: manual.id });
    check("restore takes the file back out of the Recycle Bin", back[0]?.status === "restored" && existsSync(victim) && readFileSync(victim).length === 1024 * 1024, JSON.stringify(back));
  }

  // Clean with a rule pack that only matches the fixture's junk folder: the Clean
  // view scans with it (window.__nfdRulesDir), then the commands are driven directly
  await evaluate(`window.__nfdRulesDir = ${JSON.stringify(rulesDir)}`);
  await nav(4);
  for (let i = 0; i < 80 && !(await evaluate("document.querySelector('main').innerText.includes('Smoke fixture')")); i++) {
    await evaluate("(() => { const b = document.querySelectorAll('main header .actions-top .btn')[1]; if (b && !b.disabled) b.click(); })()");
    await sleep(250);
  }
  check("Clean view shows the rule", await evaluate("document.querySelector('main').innerText.includes('Smoke fixture')"));
  await shot("9-clean");
  const cs = await invoke("clean_scan", { rulesDir });
  const rule = cs.rules?.find((r) => r.ruleId === "smoke.junk");
  check("clean scan finds the fixture junk", rule?.candidates === 2 && rule.files === JUNK_FILES && cs.safe === rule.bytes, JSON.stringify(rule ?? cs));
  const preview = await invoke("clean_items", { ruleId: "smoke.junk", limit: 10 });
  check("clean preview lists the junk", preview.length === 2 && preview.every((p) => p.path.startsWith(join(fixture, "junk"))), JSON.stringify(preview));
  const cleaned = await invoke("clean_run", { ruleIds: ["smoke.junk"], expert: false, permanent: false });
  if (cleaned.id) operations.push(cleaned.id);
  check(
    "clean moves the junk to the quarantine",
    cleaned.done === 2 && cleaned.failed === 0 && !existsSync(join(fixture, "junk", "old.log")) && !existsSync(join(fixture, "junk", "cache")),
    JSON.stringify(cleaned),
  );
  check("clean keeps the folder the rule cleans inside", existsSync(join(fixture, "junk")));
  await nav(5);
  for (let i = 0; i < 30 && !(await evaluate("document.querySelectorAll('.op').length > 0")); i++) await sleep(150);
  check("Activity view lists operations", await evaluate("document.querySelectorAll('.op').length >= 3"));
  await shot("10-activity");
  if (cleaned.id) {
    const back = await invoke("activity_restore", { id: cleaned.id });
    check(
      "restore puts the junk back byte for byte",
      back.length === 2 && back.every((r) => r.status === "restored") && readFileSync(join(fixture, "junk", "old.log"), "utf8") === "log line\n" && readFileSync(join(fixture, "junk", "cache", "a.bin")).length === 64 * 1024,
      JSON.stringify(back),
    );
  }
  const refusedRules = await invoke("clean_scan", { rulesDir: fixture });
  check("clean scan of a folder without packs finds nothing", !!refusedRules.__error || refusedRules.rules?.length === 0, JSON.stringify(refusedRules).slice(0, 120));
  await evaluate("delete window.__nfdRulesDir");

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

  // A5 Folder Encyclopedia
  const nm = await invoke("find_path", { path: "projects\\web\\node_modules" });
  const nmArticle = await invoke("explain", { id: nm, lang: "vi" });
  check("encyclopedia explains node_modules", nmArticle?.id === "node-modules" && !nmArticle.inside && nmArticle.title.length > 5, JSON.stringify(nmArticle?.title));
  const inner = await invoke("find_path", { path: "projects\\web\\node_modules\\pkg\\index.js" });
  const innerArticle = await invoke("explain", { id: inner, lang: "en" });
  check("encyclopedia covers what is inside", innerArticle?.id === "node-modules" && innerArticle.inside === true);
  // an ordinary folder only gets the article of a described parent (the fixture lives in %TEMP%)
  const plain = await invoke("explain", { id: videos?.id, lang: "en" });
  check("ordinary folder gets no article of its own", plain === null || (plain.inside && plain.id === "temp"), JSON.stringify(plain?.id ?? null));
  // A9 age heatmap and owner
  const age = await invoke("age_matrix", { id: 0, limit: 10 });
  const ageTotal = age.totals?.reduce((a, b) => a + b, 0);
  check("age heatmap adds up", age.totals?.length === 7 && ageTotal === (await invoke("summary")).stats.on_disk, `${ageTotal}`);
  const owner = await invoke("owner", { id: 0 });
  check("owner of the fixture", typeof owner === "string" && owner.length > 2, owner);
  // A3 orphan junction (read-only)
  const brokenLinks = await invoke("broken_links", { id: 0, limit: 10 });
  check("orphan junction found", brokenLinks.rows?.length === 1 && brokenLinks.rows[0].name === "old-link" && brokenLinks.rows[0].status === "missing", JSON.stringify(brokenLinks.rows?.map((r) => r.target)));
  // A7 Explorer menu: turn on, check the registry, turn off, restore the previous state
  const shellBefore = await invoke("shell_menu_status");
  const on = await invoke("shell_menu_set", { enabled: true, caption: "Analyze with NANOFreeDisk" });
  let regValue = "";
  try {
    regValue = execFileSync("reg", ["query", "HKCU\\Software\\Classes\\Directory\\shell\\NANOFreeDisk\\command", "/ve"], { encoding: "utf8" });
  } catch {
    /* missing key */
  }
  check("Explorer menu registered for this exe", on === true && regValue.toLowerCase().includes(exe.toLowerCase()) && regValue.includes("%V"), regValue.trim().split("\n").pop());
  let dupesVerb = "";
  try {
    dupesVerb = execFileSync("reg", ["query", "HKCU\\Software\\Classes\\Directory\\shell\\NANOFreeDiskDupes\\command", "/ve"], { encoding: "utf8" });
  } catch {
    /* missing key */
  }
  check("Explorer menu has \"Find duplicates here\"", dupesVerb.toLowerCase().includes(exe.toLowerCase()) && dupesVerb.includes("--dupes"), dupesVerb.trim().split("\n").pop());
  const off = await invoke("shell_menu_set", { enabled: false, caption: "" });
  let stillThere = true;
  try {
    execFileSync("reg", ["query", "HKCU\\Software\\Classes\\Directory\\shell\\NANOFreeDisk"], { stdio: "ignore" });
  } catch {
    stillThere = false;
  }
  check("Explorer menu removed", off === false && !stillThere);
  if (shellBefore) await invoke("shell_menu_set", { enabled: true, caption: "Analyze with NANOFreeDisk" });
  check("no startup path when started without one", (await invoke("take_startup_path")) === null);
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

  // Phase 4: DevCache with a pack that only matches its own fixture
  const devNav = await evaluate("[...document.querySelectorAll('.side .nav')].findIndex((b) => /Dev caches|Cache lập trình/.test(b.innerText))");
  check("Dev caches is in the sidebar", devNav >= 0);
  await evaluate(`window.__nfdDevRulesDir = ${JSON.stringify(devRulesDir)}`);
  await nav(devNav);
  for (let i = 0; i < 80 && !(await evaluate("document.querySelector('main').innerText.includes('NANO Dev')")); i++) await sleep(150);
  check("DevCache view groups the fixture vendor", await evaluate("!!document.querySelector('[data-vendor=\"NANO Dev\"]')"));
  check("DevCache view lists what is kept", await evaluate("document.querySelector('[data-vendor=\"NANO Dev\"]')?.innerText.includes('Settings of the GUI smoke test fixture') ?? false"));
  await shot("13-devcache");
  const dc = await invoke("devcache_scan", { rulesDir: devRulesDir, thresholds: { "NANO Dev": 0.00001 } });
  const dv = dc.vendors?.find((v) => v.vendor === "NANO Dev");
  check(
    "devcache scan: vendor → app → rule, keep list, threshold",
    dv?.apps?.[0]?.rules?.[0]?.ruleId === "smoke.dev.cache" && dv.apps[0].kept.length === 1 && dv.safeBytes > 0 && dv.overThreshold === true && dc.vendors.length === 1,
    JSON.stringify(dc).slice(0, 200),
  );
  const devItems = await invoke("devcache_items", { ruleId: "smoke.dev.cache", limit: 10 });
  check("devcache preview lists the fixture caches", devItems.length === 2 && devItems.every((p) => p.path.startsWith(join(devFixture, "caches"))), JSON.stringify(devItems));
  const devCleaned = await invoke("devcache_run", { ruleIds: ["smoke.dev.cache"], expert: false, permanent: false });
  if (devCleaned.id) operations.push(devCleaned.id);
  check(
    "devcache clean moves the caches to the quarantine and keeps settings",
    devCleaned.done === 2 && !existsSync(join(devFixture, "caches", "index.db")) && existsSync(join(devFixture, "keep", "settings.json")),
    JSON.stringify(devCleaned),
  );
  if (devCleaned.id) {
    const back = await invoke("activity_restore", { id: devCleaned.id });
    check("devcache restore brings the caches back", back.length === 2 && readFileSync(join(devFixture, "caches", "index.db"), "utf8") === "idx\n", JSON.stringify(back));
  }
  await evaluate("delete window.__nfdDevRulesDir");

  const repoScan = await invoke("devcache_repos", { root: join(devFixture, "projects"), keepRecentDays: 7, staleDays: 90, gitStatus: false });
  const smokeRepo = repoScan.repos?.find((r) => r.name === "smoke-app");
  const buildOut = smokeRepo?.artifacts?.find((a) => a.rel === "target");
  check("repo finder: project and its build output", !!buildOut && buildOut.selected && buildOut.level === "safe" && smokeRepo.kinds.includes("rust"), JSON.stringify(repoScan).slice(0, 240));
  if (buildOut) {
    const moved = await invoke("devcache_repos_clean", { paths: [buildOut.path] });
    if (moved.id) operations.push(moved.id);
    check("repo build output goes to the quarantine", moved.done === 1 && !existsSync(join(devProject, "target")) && existsSync(join(devProject, "Cargo.toml")), JSON.stringify(moved));
    if (moved.id) {
      const back = await invoke("activity_restore", { id: moved.id });
      check("repo build output is restored", back[0]?.status === "restored" && existsSync(join(devProject, "target", "debug", "smoke-app.exe")), JSON.stringify(back));
    }
  }
  const tools = await invoke("devcache_toolchains");
  check("toolchains: read-only inventory", Array.isArray(tools) && tools.every((x) => typeof x.family === "string" && (x.command === null || !/rmdir|del |rm -/i.test(x.command))), `${tools.length} toolchain(s)`);
  const stores = await invoke("devcache_stores");
  check("package stores: listed with exact command lines", Array.isArray(stores) && stores.every((x) => typeof x.commandLine === "string" && (x.undo === "redownload" || !x.runnable)), stores.map?.((x) => x.id).join(" "));
  const runnable = stores.find?.((x) => x.runnable);
  if (runnable) {
    const refusedRun = await invoke("devcache_store_run", { id: runnable.id, commandLine: `${runnable.commandLine} --extra` });
    check("package store command refuses a command line nobody confirmed", refusedRun.__error?.code === "command", JSON.stringify(refusedRun));
  }
  const unknownDirs = await invoke("devcache_unknown", { minMb: 50 });
  check("unknown folders are unverified and never pre-selected", Array.isArray(unknownDirs) && unknownDirs.every((f) => f.level === "caution" && f.proposeUrl.startsWith("https://github.com/congluuit/NANOFreeDisk/issues/new?")), `${unknownDirs.length} folder(s)`);
  const badReveal = await invoke("devcache_reveal", { path: "C:\\Windows\\System32" });
  check("reveal only opens folders the view showed", badReveal.__error?.code === "not-found", JSON.stringify(badReveal));
  const badUrl = await invoke("devcache_propose", { url: "https://example.com/" });
  check("propose only opens the rule-proposal form", badUrl.__error?.code === "bad-url", JSON.stringify(badUrl));
  for (const tab of ["toolchains", "repos", "stores", "unknown"]) {
    await evaluate(`document.querySelector('[data-devtab=${tab}]').click()`);
    await sleep(400);
  }
  check("DevCache tabs render", await evaluate("!!document.querySelector('[data-devtab=unknown][aria-selected=true]')"));

  // AI leftovers (C10): the tab and the commands, on the AI fixture only
  await evaluate("document.querySelector('[data-devtab=ai]').click()");
  // let a background scan of Home / the tab finish before pointing it at the fixture
  for (let i = 0; i < 300 && (await evaluate("!!document.querySelector('.devcache .dc-loading')")); i++) await sleep(100);
  await evaluate(`window.__nfdAiFixture = ${JSON.stringify(aiFixture)}`);
  await evaluate("[...document.querySelectorAll('.devcache button')].find((b) => /Scan again|Quét lại/.test(b.innerText))?.click()");
  for (let i = 0; i < 100 && !(await evaluate("!!document.querySelector('[data-ai-item=\".playwright-mcp\"]')")); i++) await sleep(150);
  check("AI leftovers tab lists the fixture's agent output", await evaluate("!!document.querySelector('[data-ai-item=\".playwright-mcp\"]')"));
  check("AI leftovers tab shows conversation history as kept", await evaluate("!!document.querySelector('[data-ai-kept=\".aider.chat.history.md\"]')"));
  await shot("13b-ai-leftovers");
  const ai = await invoke("ai_leftovers_scan", { roots: [], keepRecentDays: 7, fixture: aiFixture });
  const aiItem = (rel) => ai.items?.find((i) => i.rel === rel);
  const doneScratch = aiItem(`claude/smoke-proj/${SID_DONE}`);
  const openScratch = aiItem(`claude/smoke-proj/${SID_OPEN}`);
  check(
    "AI leftovers: finished session pre-selected, open session kept",
    doneScratch?.selected === true && doneScratch.session?.title === "Smoke finished session" && openScratch?.selected === false && openScratch.keep === "session-open",
    JSON.stringify(ai.items?.map((i) => [i.rel, i.selected, i.keep])),
  );
  check(
    "AI leftovers: newest Playwright browser and its MCP profile stay",
    aiItem("chromium-1000")?.selected === true && !aiItem("chromium-1001") && !aiItem("mcp-chrome-smoke"),
  );
  check("AI leftovers: chat history is never an item", !ai.items?.some((i) => /aider\.chat|\.claude[\\/]projects/i.test(i.path)) && ai.kept?.some((k) => k.what === ".aider.chat.history.md"));
  const wtItem = aiItem(".claude/worktrees/smoke-done");
  if (aiGit) check("AI leftovers: clean merged worktree can be removed by git", wtItem?.worktree?.removable === true && !wtItem.blocked, JSON.stringify(wtItem));
  const aiPaths = [doneScratch?.path, aiItem(".playwright-mcp")?.path, aiItem("chromium-1000")?.path, aiGit ? wtItem?.path : null].filter(Boolean);
  const aiPlan = await invoke("ai_leftovers_plan", { paths: aiPaths });
  check("AI leftovers plan shows the git command", !aiGit || aiPlan.commands?.some((c) => c.includes("worktree remove")), JSON.stringify(aiPlan));
  const aiDone = await invoke("ai_leftovers_clean", { paths: aiPaths });
  const aiOps = (await invoke("activity_list", { limit: 6 })).filter?.((o) => o.kind === "ai-leftovers" || o.kind === "worktree") ?? [];
  for (const o of aiOps) operations.push(o.id);
  check(
    "AI leftovers clean: folders to the quarantine, history and the open session untouched",
    aiDone.failed === 0 && !existsSync(join(aiProject, ".playwright-mcp")) && existsSync(join(aiProject, ".aider.chat.history.md")) && existsSync(openScratch?.path ?? "") && !existsSync(doneScratch?.path ?? ""),
    JSON.stringify(aiDone),
  );
  if (aiGit) {
    const branch = execFileSync("git", ["-C", aiProject, "branch", "--list", "smoke/done"], { encoding: "utf8" });
    check("AI leftovers: git removed the worktree, the branch stays", !existsSync(aiWorktree) && branch.includes("smoke/done"));
    const wtOp = aiOps.find((o) => o.kind === "worktree");
    const wtDetail = wtOp ? await invoke("activity_items", { id: wtOp.id }) : null;
    check("Activity keeps how to add the worktree back", !!wtDetail?.items?.[0]?.message?.includes("worktree add"), JSON.stringify(wtDetail?.items?.[0]));
  }
  const aiQ = aiOps.find((o) => o.kind === "ai-leftovers");
  if (aiQ) {
    const back = await invoke("activity_restore", { id: aiQ.id });
    check("AI leftovers restore brings the folders back", Array.isArray(back) && existsSync(join(aiProject, ".playwright-mcp", "console-1.log")) && existsSync(doneScratch?.path ?? ""), JSON.stringify(back).slice(0, 300));
  }
  await evaluate("delete window.__nfdAiFixture");
  await evaluate("document.querySelector('[data-devtab=cache]').click()");

  // Phase 5: Duplicates (own fixture, hash cache off so nothing reaches the real DB)
  await evaluate("window.__nfdNoDupeCache = true");
  const noCache = { minSize: 1024 * 1024, includeAppDirs: false, paranoid: false, extensions: [], useCache: false };
  const dupesFound = await invoke("dupes_find", { paths: [dupeFixture], options: noCache });
  const video = dupesFound.groups?.find((g) => g.size === dupeA.length);
  check(
    "dupes: groups by content, small files and node_modules left out",
    dupesFound.totalGroups === 2 && video?.copies.length === 3 && dupesFound.wastedBytes === 2 * dupeA.length + dupeB.length && dupesFound.skipped?.["app-dir"] === 1 && dupesFound.skipped?.small === 2,
    JSON.stringify({ groups: dupesFound.totalGroups, wasted: dupesFound.wastedBytes, skipped: dupesFound.skipped ?? dupesFound }).slice(0, 240),
  );
  const sel = await invoke("dupes_select", { strategy: "auto" });
  const videoSel = sel.find?.((s) => s.group === video?.id);
  check("dupes: keeps the copy outside Downloads", !!videoSel && videoSel.keep.endsWith(join("Photos", "2019", "video.mp4")) && videoSel.remove.length === 2, JSON.stringify(sel));
  const emptied = await invoke("dupes_run", { selections: [{ group: video?.id, keep: videoSel?.keep, remove: video?.copies.map((c) => c.path) }], action: "quarantine" });
  check("dupes: a selection that keeps nothing is refused", emptied.__error?.code === "dupes-plan" && existsSync(join(dupeFixture, "Photos", "2019", "video.mp4")), JSON.stringify(emptied));
  const thumb = await invoke("dupes_thumbnail", { path: join(dupeFixture, "Photos", "pic.png") });
  check("dupes: image preview from the backend", typeof thumb === "string" && thumb.startsWith("data:image/png;base64,"), String(thumb).slice(0, 40));
  const badThumb = await invoke("dupes_thumbnail", { path: "C:\\Windows\\Web\\Wallpaper\\Windows\\img0.jpg" });
  const badDupeReveal = await invoke("dupes_reveal", { path: "C:\\Windows\\System32" });
  check("dupes: preview and reveal only for paths of the search", badThumb.__error?.code === "not-found" && badDupeReveal.__error?.code === "not-found");
  const csvPath = join(dupeFixture, "dupes.csv");
  await invoke("dupes_export", { path: csvPath, format: "csv" });
  const badExport = await invoke("dupes_export", { path: join(dupeFixture, "x.exe"), format: "csv" });
  check("dupes: CSV export (and nothing but .csv / .json)", existsSync(csvPath) && readFileSync(csvPath, "utf8").split("\n").length >= 6 && badExport.__error?.code === "bad-path");
  rmSync(csvPath, { force: true });
  const removed = await invoke("dupes_run", { selections: sel, action: "quarantine" });
  if (removed.id) operations.push(removed.id);
  check(
    "dupes: copies go to the quarantine, one of each group stays",
    removed.done === 3 && existsSync(join(dupeFixture, "Photos", "2019", "video.mp4")) && existsSync(join(dupeFixture, "Photos", "pic.png")) && !existsSync(join(dupeFixture, "Downloads", "video.mp4")),
    JSON.stringify(removed),
  );
  if (removed.id) {
    const back = await invoke("activity_restore", { id: removed.id, items: null });
    check("dupes: restore brings the copies back", back.length === 3 && readFileSync(join(dupeFixture, "Downloads", "video (1).mp4")).equals(dupeA), JSON.stringify(back));
  }
  // the keeper changes after the search: its group is left alone
  const again = await invoke("dupes_find", { paths: [dupeFixture], options: noCache });
  const sel2 = await invoke("dupes_select", { strategy: "auto" });
  const keeper = sel2.find((s) => s.remove.some((p) => p.endsWith("video.mp4")))?.keep;
  if (keeper) writeFileSync(keeper, randomBytes(dupeA.length));
  const guarded = await invoke("dupes_run", { selections: sel2, action: "quarantine" });
  if (guarded.id) operations.push(guarded.id);
  check(
    "dupes: a keeper that changed protects its whole group",
    again.totalGroups === 2 && guarded.problems?.filter((p) => p.reason === "keeper-changed").length === 2 && existsSync(join(dupeFixture, "Downloads", "video.mp4")),
    JSON.stringify(guarded.problems ?? guarded).slice(0, 200),
  );
  // duplicate folders: two identical albums, each next to something unique
  const folderDir = join(dupeFixture, "folders");
  for (const side of ["A", "B"]) {
    mkdirSync(join(folderDir, side, "Trip", "raw"), { recursive: true });
    writeFileSync(join(folderDir, side, "Trip", "a.jpg"), dupeB);
    writeFileSync(join(folderDir, side, "Trip", "raw", "b.dng"), dupeA.subarray(0, 4096));
    writeFileSync(join(folderDir, side, "Trip", "notes.txt"), "trip\n");
    writeFileSync(join(folderDir, side, `only-${side}.txt`), side);
  }
  const folderFound = await invoke("dupes_find", { paths: [folderDir], options: { ...noCache, folders: true } });
  const trip = folderFound.folderGroups?.find((g) => g.folders.every((f) => f.path.endsWith("Trip")));
  check("dupes: identical folders are found at the top level", !!trip && trip.folders.length === 2 && trip.files === 3 && folderFound.folderGroups.length === 1, JSON.stringify(folderFound.folderGroups ?? folderFound).slice(0, 300));
  const folderSel = await invoke("dupes_select", { strategy: "auto", folders: true });
  const movedFolder = await invoke("dupes_run", { selections: folderSel, action: "quarantine", folders: true });
  if (movedFolder.id) operations.push(movedFolder.id);
  const goneSide = folderSel[0]?.remove[0] ?? "";
  check("dupes: a duplicate folder goes whole, the other stays", movedFolder.done === 1 && !existsSync(goneSide) && existsSync(folderSel[0]?.keep ?? ""), JSON.stringify(movedFolder));
  if (movedFolder.id) {
    const back = await invoke("activity_restore", { id: movedFolder.id, items: null });
    check("dupes: the folder comes back", back[0]?.status === "restored" && existsSync(join(goneSide, "raw", "b.dng")), JSON.stringify(back));
  }
  const healthDupes = await invoke("health");
  check("health mentions the duplicates found", healthDupes.findings?.some((f) => f.kind === "duplicates" && f.fix === "duplicates"), JSON.stringify(healthDupes.findings?.map((f) => f.kind)));
  // replace a duplicate by a hard link (NTFS temp folder), then undo it
  const linkDir = join(dupeFixture, "links");
  mkdirSync(join(linkDir, "Downloads"), { recursive: true });
  const disk = randomBytes(1100 * 1024);
  writeFileSync(join(linkDir, "setup.iso"), disk);
  writeFileSync(join(linkDir, "Downloads", "setup.iso"), disk);
  utimesSync(join(linkDir, "setup.iso"), yearAgo, yearAgo);
  const linkFound = await invoke("dupes_find", { paths: [linkDir], options: noCache });
  const linkGroup = linkFound.groups?.[0];
  if (linkGroup?.copies.every((c) => c.link === "hardlink")) {
    const linked = await invoke("dupes_run", {
      selections: [{ group: linkGroup.id, keep: join(linkDir, "setup.iso"), remove: [join(linkDir, "Downloads", "setup.iso")] }],
      action: "link",
    });
    if (linked.id) operations.push(linked.id);
    check(
      "dupes: a copy becomes a hard link to the one kept",
      linked.done === 1 && statSync(join(linkDir, "Downloads", "setup.iso")).nlink === 2 && readFileSync(join(linkDir, "Downloads", "setup.iso")).equals(disk),
      JSON.stringify(linked),
    );
    if (linked.id) {
      const back = await invoke("activity_restore", { id: linked.id, items: null });
      check(
        "dupes: restore puts the original back, two independent files again",
        back[0]?.status === "restored" && statSync(join(linkDir, "Downloads", "setup.iso")).nlink === 1 && statSync(join(linkDir, "setup.iso")).nlink === 1,
        JSON.stringify(back),
      );
    }
  } else {
    check("dupes: temp drive supports links", false, JSON.stringify(linkGroup?.copies.map((c) => c.link)));
  }
  // the view: search the current scan of the fixture from the UI
  await scan(dupeFixture, false);
  const dupNav = await evaluate("[...document.querySelectorAll('.side .nav')].findIndex((b) => /Duplicates|Tệp trùng/.test(b.innerText))");
  check("Duplicates is in the sidebar", dupNav >= 0);
  await nav(dupNav);
  await sleep(500);
  await evaluate("document.querySelector('[data-find]')?.click()");
  for (let i = 0; i < 40 && !(await evaluate("!!document.querySelector('[data-dupes-summary]')")); i++) await sleep(250);
  check("Duplicates view lists the groups", await evaluate("document.querySelectorAll('.group').length >= 1 && document.querySelector('[data-dupes-summary]').innerText.length > 10"));
  await shot("14-duplicates");
  await evaluate("document.querySelector('[data-recycle]')?.click()");
  await sleep(300);
  check("Duplicates asks before moving anything", await evaluate("!!document.querySelector('.dialog [data-confirm]')"));
  await evaluate("document.querySelector('.dialog footer .btn').click()");
  await sleep(200);
  await evaluate("document.querySelector('[data-link]')?.click()");
  await sleep(300);
  check("Replace with links explains the shared content first", await evaluate("!!document.querySelector('.dialog [data-confirm]') && document.querySelector('.dialog').innerText.length > 80"));
  await evaluate("document.querySelector('.dialog footer .btn').click()");
  await sleep(200);
  await evaluate("document.querySelector('[data-dupetab=folders]')?.click()");
  await sleep(200);

  // Monitor (docs/PHASE6_MONITOR.md): I/O counters only — file detail would ask UAC.
  // Minutes are not recorded into the real history from here.
  const noAlerts = { enabled: false, mbps: 5, minutes: 5 };
  const monTexts = { spamTitle: "", spamBody: "", logTitle: "", logBody: "" };
  check("monitor: configure", (await invoke("monitor_configure", { alerts: noAlerts, texts: monTexts, record: false }))?.__error === undefined);
  await invoke("monitor_snapshot");
  // this script writes a file for a few seconds: its own process must show up
  const monFile = join(fixture, "monitor-write.bin");
  const chunk = randomBytes(1 << 20);
  let mine = null;
  let monView = null;
  for (let i = 0; i < 16 && !mine; i++) {
    writeFileSync(monFile, Buffer.concat([chunk, chunk, chunk, chunk]));
    await sleep(350);
    monView = await invoke("monitor_snapshot");
    mine = monView.snapshot?.processes?.find((p) => p.pid === process.pid && p.write > 0);
  }
  const monSnap = monView?.snapshot;
  check(
    "monitor: samples processes and disks without admin",
    monSnap?.schema === "nanofreedisk.monitor.v1" && monSnap.source === "counters" && Array.isArray(monSnap.disks) && monSnap.processes.length > 0,
    `${monSnap?.processes?.length} active, ${monSnap?.disks?.length} disk(s), elevated ${monView?.elevated}`,
  );
  check("monitor: this script's writes are attributed to it", !!mine, mine ? `${(mine.write / 2 ** 20).toFixed(1)} MB/s` : "not seen");
  const monChart = await invoke("monitor_chart", { seconds: 60 });
  check("monitor: realtime chart", Array.isArray(monChart) && monChart.length >= 2 && monChart.every((p) => p.t > 0), `${monChart.length} points`);
  const monRefused = await invoke("monitor_reveal", { path: "C:\\Windows\\System32\\config\\SAM" });
  check("monitor: Explorer opens only paths it reported", monRefused?.__error?.code === "not-found", JSON.stringify(monRefused));
  check("monitor: 24-hour history reads", Array.isArray((await invoke("monitor_history")).minutes));
  check("monitor: pause", (await invoke("monitor_pause", { paused: true })) === true && (await invoke("monitor_snapshot")).paused === true);
  await invoke("monitor_pause", { paused: false });
  const monNav = await evaluate("[...document.querySelectorAll('.side .nav')].findIndex((b) => /Disk activity|Hoạt động ổ đĩa/.test(b.innerText))");
  check("Disk activity is in the sidebar", monNav >= 0);
  await nav(monNav);
  for (let i = 0; i < 30 && !(await evaluate("document.querySelectorAll('[data-procs] .row').length > 0")); i++) await sleep(200);
  await sleep(1500);
  check("Monitor view lists processes and draws the chart", await evaluate("document.querySelectorAll('[data-procs] .row').length > 0 && (document.querySelector('[data-chart] path.line')?.getAttribute('d') ?? '').length > 4"));
  check("Monitor view shows the physical disks", await evaluate("document.querySelectorAll('[data-disk]').length > 0"));
  await shot("15-monitor");
  await evaluate("document.querySelector('[data-montab=history]')?.click()");
  for (let i = 0; i < 20 && !(await evaluate("!!document.querySelector('[data-history]')")); i++) await sleep(150);
  check("Monitor: 24 hours tab", await evaluate("!!document.querySelector('[data-history]')"));
  await evaluate("document.querySelector('[data-montab=who]')?.click()");
  await sleep(300);
  check("Monitor: Who touched this? tab", await evaluate("!!document.querySelector('[data-who]')"));
  await evaluate("document.querySelector('[data-montab=activity]')?.click()");

  // Phase 7: Health (read-only; the export goes to the temp fixture folder)
  const driveHealth = await invoke("health_report", { refresh: true });
  check(
    "health: report lists this PC's drives and volumes",
    driveHealth.schema === "nanofreedisk.health.v1" && driveHealth.drives?.length >= 1 && driveHealth.volumes?.length >= 1 &&
      driveHealth.drives.every((d) => ["unknown", "good", "caution", "bad"].includes(d.status)),
    JSON.stringify(driveHealth).slice(0, 300),
  );
  const healthJson = join(fixture, "health.json");
  await invoke("health_export", { path: healthJson, serials: false });
  const exported = existsSync(healthJson) ? JSON.parse(readFileSync(healthJson, "utf8")) : null;
  check("health: JSON export leaves serial numbers out", exported?.drives?.every((d) => d.serial == null), JSON.stringify(exported?.drives?.map((d) => d.serial)));
  const badHealthExport = await invoke("health_export", { path: join(fixture, "health.exe"), serials: false });
  check("health: export only writes *.json", badHealthExport?.__error?.code === "bad-path", JSON.stringify(badHealthExport));
  const healthNav = await evaluate("[...document.querySelectorAll('.side .nav')].findIndex((b) => /Drive health|Sức khỏe ổ đĩa/.test(b.innerText))");
  check("Health is in the sidebar", healthNav >= 0);
  await nav(healthNav);
  for (let i = 0; i < 40 && !(await evaluate("!!document.querySelector('[data-drive]')")); i++) await sleep(250);
  check("Health view shows a card per drive", await evaluate(`document.querySelectorAll('[data-drive]').length === ${driveHealth.drives.length}`));
  check("Health view shows the volume checks", await evaluate("document.querySelectorAll('[data-volume]').length >= 1 && !!document.querySelector('[data-detail]')"));
  await shot("15-health");
  const driveScore = await invoke("health");
  check("health score knows the drives", driveScore.findings?.every((f) => typeof f.kind === "string"), JSON.stringify(driveScore.findings?.map((f) => f.kind)));

  // G4: every control that changes something explains what happens
  const untitled = [];
  for (const [i, view] of [[0, "home"], [1, "analyze"], [2, "oldlarge"], [3, "tidy"], [4, "clean"], [5, "activity"], [devNav, "devcache"], [dupNav, "duplicates"], [monNav, "monitor"], [healthNav, "health"]]) {
    await nav(i);
    await sleep(600);
    const missing = await evaluate("[...document.querySelectorAll('[data-effect]')].filter((e) => !(e.getAttribute('title') ?? '').trim()).map((e) => e.outerHTML.slice(0, 60))");
    if (missing.length) untitled.push(`${view}: ${missing.join(" | ")}`);
  }
  await evaluate("[...document.querySelectorAll('.side .nav')].pop().click()");
  await sleep(600);
  const settingsMissing = await evaluate("[...document.querySelectorAll('[data-effect]')].filter((e) => !(e.getAttribute('title') ?? '').trim()).length");
  const effects = await evaluate("document.querySelectorAll('[data-effect]').length");
  check("every changing control has an 'If you do this' tooltip", !untitled.length && settingsMissing === 0 && effects > 5, untitled.join("; ") || `${effects} on Settings`);
  await shot("9-settings");

  // A7 single instance: a second launch with a folder hands it to this window
  const videosDir = join(fixture, "videos");
  spawn(exe, [videosDir], { stdio: "ignore", env: process.env }).unref();
  let handed = false;
  for (let i = 0; i < 80 && !handed; i++) {
    await sleep(150);
    const sum = await invoke("summary");
    handed = !sum.__error && sum.root.toLowerCase() === videosDir.toLowerCase();
  }
  check("second launch with a folder opens it in the running window", handed);
  await invoke("history_forget", { root: videosDir });
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
  const th = await invoke("thermal_set", { enabled: false, texts: { title: "t", body: "b" } });
  check("temperature log and alert switch", th.enabled === false && Array.isArray(th.minutes), `${th.minutes?.length} logged minutes`);
  const healthNow = await invoke("health");
  check("health score includes heat findings when any", Number.isInteger(healthNow.score) && healthNow.findings.every((f) => !f.kind.startsWith("hot-") || f.fix === "hardware"));
  const adv = await invoke("pawnio_status");
  check("Advanced sensors status needs no admin", typeof adv.active === "boolean" && !adv.__error, JSON.stringify(adv.pawnio ?? adv));
  await evaluate("document.querySelector('[data-hwtab=sensors]').click()");
  for (let i = 0; i < 40 && !(await evaluate("!!document.querySelector('[data-sensor=\"cpu/cpu-total\"]')")); i++) await sleep(150);
  check("Sensors tab shows CPU Total", await evaluate("!!document.querySelector('[data-sensor=\"cpu/cpu-total\"]')"));
  check("Sensors tab shows the 24-hour card", await evaluate("!!document.querySelector('.t24 [data-alerts]')"));
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
    for (const id of operations) await invoke("activity_forget", { id });
  } catch {
    /* app already gone */
  }
  try {
    await invoke("history_forget", { root: dupeFixture });
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
  rmSync(rulesDir, { recursive: true, force: true });
  rmSync(devFixture, { recursive: true, force: true });
  rmSync(devRulesDir, { recursive: true, force: true });
  rmSync(dupeFixture, { recursive: true, force: true });
  rmSync(aiFixture, { recursive: true, force: true });
}

if (failures.length) {
  console.error(`gui-smoke: ${failures.length} failure(s)`);
  process.exit(1);
}
console.log("gui-smoke: all checks passed");
