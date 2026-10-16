#!/usr/bin/env node
// Records the Phase 2 demo of the real desktop app: builds a synthetic "demo drive"
// (no personal files are ever shown), starts the built exe on it, walks
// through the features while capturing frames with the WebView2 DevTools screencast,
// then turns the frames into docs/assets/demo/ animations (scripts/frames-to-video.py:
// GIF + WebP with Pillow, MP4 too when ffmpeg is on PATH).
//
//   node scripts/demo-video.mjs [path\to\nanofreedisk.exe] [--keep-frames]
//
// The user's settings, theme and scan history are restored afterwards; the demo
// folder is deleted. Windows only.

import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { dirname, join, parse, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const keepFrames = args.includes("--keep-frames");
const exe = resolve(args.find((a) => !a.startsWith("--")) ?? join(root, "target", "release", "nanofreedisk.exe"));
const PORT = 9833 + Math.floor(Math.random() * 300);
const MB = 1024 * 1024;
const DAY = 86_400_000;

if (process.platform !== "win32") {
  console.log("demo-video: Windows only, skipped");
  process.exit(0);
}
if (!existsSync(exe)) {
  console.error(`demo-video: ${exe} not found (build with: pnpm --dir apps/nanofreedisk tauri build --no-bundle)`);
  process.exit(2);
}

// ── synthetic demo drive ─────────────────────────────────────────────────────
// A fresh folder at the root of the repo's drive (removed afterwards): not under
// %TEMP% or a project, so no encyclopedia article covers it, and no user name shows
// in its path. Frames go to target/demo (git-ignored).
const demo = mkdtempSync(join(parse(root).root, "DemoPC-"));
mkdirSync(join(root, "target", "demo"), { recursive: true });
const base = mkdtempSync(join(root, "target", "demo", "run-"));
const frames = join(base, "frames");
mkdirSync(frames);
let seed = 7;
const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31);
function file(rel, size, ageDays = rnd() * 900) {
  const p = join(demo, rel);
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, Buffer.alloc(Math.round(size), 1 + Math.floor(rnd() * 200)));
  const t = new Date(Date.now() - ageDays * DAY);
  utimesSync(p, t, t);
}
file("Downloads/Win11_23H2_English_x64.iso", 140 * MB, 700);
file("Downloads/old-game-setup.exe", 38 * MB, 420);
file("Downloads/drivers-2023.zip", 22 * MB, 600);
file("Downloads/invoice-march.pdf", 0.4 * MB, 20);
file("Videos/Trip to Da Lat 2024.mp4", 96 * MB, 500);
file("Videos/drone-footage.mov", 64 * MB, 160);
file("Videos/birthday.mp4", 28 * MB, 40);
for (let i = 0; i < 10; i++) file(`Pictures/RAW/IMG_${4100 + i}.CR3`, 7 * MB, 300 + i * 30);
for (let i = 0; i < 24; i++) file(`Pictures/Phone/PXL_2026${String(i).padStart(4, "0")}.jpg`, 2.4 * MB, i * 12);
file(".ollama/models/blobs/sha256-6a0746a1ec1aef3e", 88 * MB, 90);
file(".ollama/models/manifests/registry.ollama.ai/library/llama3/latest", 0.01 * MB, 90);
for (let i = 0; i < 160; i++) file(`Projects/web-shop/node_modules/pkg-${i % 40}/dist/index-${i}.js`, (20 + rnd() * 160) * 1024, 30);
for (let i = 0; i < 20; i++) file(`Projects/web-shop/src/components/Comp${i}.svelte`, 6 * 1024, 3 + i);
file("Projects/web-shop/package.json", 2 * 1024, 3);
for (let i = 0; i < 12; i++) file(`Projects/space-game/Library/Artifacts/${i}/asset-${i}.bin`, 3.5 * MB, 60);
for (let i = 0; i < 8; i++) file(`Projects/space-game/Assets/Textures/tex-${i}.png`, 1.8 * MB, 200);
file("Projects/space-game/ProjectSettings/ProjectSettings.asset", 0.05 * MB, 200);
for (let i = 0; i < 9; i++) file(`Projects/rust-tool/target/debug/deps/lib-${i}.rlib`, 3.2 * MB, 10);
file("Projects/rust-tool/Cargo.toml", 1024, 10);
for (let i = 0; i < 14; i++) file(`Documents/Reports/report-${2019 + (i % 7)}-${i}.pdf`, (0.5 + rnd() * 3) * MB, 2200 - i * 120);
mkdirSync(join(demo, "Documents", "Old Project", "assets", "empty"), { recursive: true });
mkdirSync(join(demo, "Documents", "New folder"), { recursive: true });
file("AppData/Local/Temp/~setup-cache.tmp", 12 * MB, 400);
// a shortcut whose target is gone (LinkInfo with a local path)
{
  const target = Buffer.from(`${join(demo, "Games", "OldGame", "game.exe")}\0\0`, "latin1");
  const head = Buffer.alloc(0x4c);
  head.writeUInt32LE(0x4c, 0);
  Buffer.from("0114020000000000c000000000000046", "hex").copy(head, 4);
  head.writeUInt32LE(2, 0x14);
  const info = Buffer.alloc(0x1c);
  [0x1c + target.length, 0x1c, 1, 0, 0x1c, 0, 0x1c + target.length - 1].forEach((v, i) => info.writeUInt32LE(v, i * 4));
  mkdirSync(join(demo, "Desktop"), { recursive: true });
  writeFileSync(join(demo, "Desktop", "Old Game.lnk"), Buffer.concat([head, info, target, Buffer.alloc(4)]));
}

// ── DevTools plumbing ────────────────────────────────────────────────────────
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let ws;
let seq = 0;
const pending = new Map();
const shots = [];
let recording = false;
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
const click = (selector) => evaluate(`(() => { const e = document.querySelector(${JSON.stringify(selector)}); e?.click(); return !!e; })()`);
const clickText = (selector, text) =>
  evaluate(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find((x) => x.innerText.includes(${JSON.stringify(text)})); e?.click(); return !!e; })()`);
const until = async (expr, ms = 15_000) => {
  for (let t = 0; t < ms; t += 150) {
    if (await evaluate(expr)) return true;
    await sleep(150);
  }
  return false;
};
async function type(selector, text) {
  for (let i = 1; i <= text.length; i++) {
    await evaluate(`(() => { const e = document.querySelector(${JSON.stringify(selector)}); e.value = ${JSON.stringify(text.slice(0, i))}; e.dispatchEvent(new Event('input', { bubbles: true })); })()`);
    await sleep(110);
  }
}
async function hover(x, y) {
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
}

// known look for the recording (light theme, English, Simple mode): the app's
// settings file is swapped for the run and restored afterwards
const settingsFile = join(process.env.APPDATA ?? "", "vn.io.nanodev.nanofreedisk", "settings.json");
const settingsBackup = join(base, "settings.json.bak");
const hadSettings = existsSync(settingsFile);
if (hadSettings) copyFileSync(settingsFile, settingsBackup);
mkdirSync(dirname(settingsFile), { recursive: true });
writeFileSync(
  settingsFile,
  JSON.stringify({ locale: "en", theme: { mode: "light", preset: "nano-light", overrides: {} }, fontScale: 1, viz: "treemap", mode: "simple", history: true }),
);

const child = spawn(exe, [demo], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  stdio: "ignore",
});
let ok = false;
try {
  let page;
  for (let i = 0; i < 80 && !page; i++) {
    try {
      page = (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).find((t) => t.type === "page");
    } catch {
      /* not up yet */
    }
    if (!page) await sleep(250);
  }
  if (!page) throw new Error("WebView2 debugging endpoint did not come up");
  ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r, j) => {
    ws.onopen = r;
    ws.onerror = j;
  });
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    if (msg.method === "Page.screencastFrame") {
      const { data, metadata, sessionId } = msg.params;
      if (recording) {
        const name = `f${String(shots.length).padStart(5, "0")}.jpg`;
        writeFileSync(join(frames, name), Buffer.from(data, "base64"));
        shots.push({ file: name, t: metadata.timestamp ?? Date.now() / 1000 });
      }
      ws.send(JSON.stringify({ id: ++seq, method: "Page.screencastFrameAck", params: { sessionId } }));
      return;
    }
    const p = pending.get(msg.id);
    if (p) {
      pending.delete(msg.id);
      msg.error ? p.rej(new Error(msg.error.message)) : p.res(msg.result);
    }
  };
  await until("!!window.__TAURI_INTERNALS__ && !!document.querySelector('.side')");
  await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
  await send("Page.enable");
  await send("Page.startScreencast", { format: "jpeg", quality: 82, maxWidth: 1280, maxHeight: 800, everyNthFrame: 1 });
  recording = true;

  // 1. the app opened on the demo folder (as from Explorer's "Analyze with NANOFreeDisk")
  await until("!!document.querySelector('.tree .row')", 30_000);
  await sleep(2200);
  await hover(900, 300);
  await sleep(1200);
  await hover(1050, 420);
  await sleep(1200);
  // 2. Ctrl+K: find node_modules and open it — the Details panel explains what it is
  //    (Folder Encyclopedia)
  await evaluate("window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))");
  await sleep(500);
  await type(".palette input", "node_mod");
  await until("[...document.querySelectorAll('.palette .item')].some((x) => x.innerText.includes('node_modules'))", 5000);
  await sleep(900);
  await evaluate("document.querySelector('.palette input').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))");
  await sleep(2600);
  await click(".crumbs .crumb");
  await sleep(1200);
  // 3. Age heatmap
  await click("[data-tab=age]");
  await until("!!document.querySelector('.age table')", 8000);
  await sleep(2600);
  // 4. Old & large → old installers
  await evaluate("document.querySelectorAll('.side .nav')[2].click()");
  await sleep(1800);
  await evaluate("document.querySelectorAll('.tabs .tab')[2]?.click()");
  await sleep(2200);
  // 5. Tidy up
  await evaluate("document.querySelectorAll('.side .nav')[3].click()");
  await sleep(2600);
  // 6. a week later: a big new video; rescan and ask "What changed?"
  file("Videos/new-4k-recording.mp4", 150 * MB, 0);
  await evaluate("document.querySelectorAll('.side .nav')[1].click()");
  await sleep(800);
  const filesBefore = (await invoke("summary")).stats.files;
  await evaluate("document.querySelectorAll('.toolbar [data-effect]')[0].click()");
  for (let i = 0; i < 200; i++) {
    await sleep(150);
    const sum = await invoke("summary");
    if (!sum.__error && sum.stats.files > filesBefore) break;
  }
  await until("!!document.querySelector('.tree .row')", 15_000);
  await sleep(1500);
  const snaps = await invoke("history_snapshots", { root: demo });
  console.log(`demo-video: ${Array.isArray(snaps) ? snaps.length : JSON.stringify(snaps)} saved scans of the demo folder`);
  await click("[data-tab=changes]");
  await until("document.querySelector('.changes')?.innerText.includes('new-4k-recording') ?? false", 8000);
  await sleep(3000);
  // 7. Home: health score; Vietnamese + dark theme in one click each
  await evaluate("document.querySelectorAll('.side .nav')[0].click()");
  await until("!!document.querySelector('.health .num b')", 8000);
  await sleep(2600);
  await evaluate("[...document.querySelectorAll('.side .nav')].pop().click()");
  await sleep(1000);
  await evaluate(`(() => { const s = document.querySelector('select'); s.value = 'vi'; s.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  await sleep(900);
  await evaluate("document.querySelectorAll('.modes .mode')[1].click()");
  await sleep(1400);
  await evaluate("document.querySelectorAll('.side .nav')[0].click()");
  await sleep(2600);
  await evaluate("document.querySelectorAll('.side .nav')[1].click()");
  await sleep(2400);
  recording = false;
  await send("Page.stopScreencast");
  ok = shots.length > 30;
  console.log(`demo-video: ${shots.length} frames`);
} catch (e) {
  console.error(`demo-video: ${e.message}`);
} finally {
  try {
    await invoke("history_forget", { root: demo });
  } catch {
    /* app gone */
  }
  try {
    ws?.close();
  } catch {
    /* ignore */
  }
  child.kill();
  await sleep(800);
  if (hadSettings) copyFileSync(settingsBackup, settingsFile);
  else rmSync(settingsFile, { force: true });
}

rmSync(demo, { recursive: true, force: true });
if (ok) {
  writeFileSync(join(frames, "frames.json"), JSON.stringify(shots));
  const out = join(root, "docs", "assets", "demo", "nanofreedisk-demo");
  mkdirSync(dirname(out), { recursive: true });
  try {
    execFileSync("python", [join(root, "scripts", "frames-to-video.py"), frames, out], { stdio: "inherit" });
  } catch (e) {
    console.error(`demo-video: frames-to-video failed (${e.message}); frames are in ${frames}`);
    ok = false;
  }
}
if (keepFrames) console.log(`frames kept in ${frames}`);
else if (ok) rmSync(base, { recursive: true, force: true });
process.exit(ok ? 0 : 1);
