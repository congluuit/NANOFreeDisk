#!/usr/bin/env node
// Renders scripts/promo-video/index.html into an MP4 (1920×1080) frame by frame:
// headless Chrome/Edge seeks the page to each frame's time over the DevTools
// protocol and the JPEG screenshots are piped into ffmpeg.
//
//   node scripts/promo-video/render.mjs [--fps 30] [--out target/promo/x.mp4] [--audio music.wav]
//   node scripts/promo-video/render.mjs --stills 3,12.5,40   # PNG stills for review
//   node scripts/promo-video/render.mjs --cues target/promo/cues.json   # sound cues for music.py
//
// ffmpeg: $FFMPEG, ffmpeg on PATH, or the binary of the imageio-ffmpeg Python package.

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..", "..");
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf("--" + name); return i >= 0 ? args[i + 1] : def; };
const fps = +opt("fps", 30);
const out = resolve(opt("out", join(root, "target", "promo", "NANOFreeDisk-promo-EN-VI.mp4")));
const audio = opt("audio", null);
const stills = opt("stills", null);
const cuesOut = opt("cues", null);
const from = +opt("from", 0);
const to = opt("to", null);

function findBrowser() {
  const c = [
    process.env.CHROME,
    "C:/Program Files/Google/Chrome/Application/chrome.exe",
    "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  ].filter(Boolean);
  const hit = c.find((p) => existsSync(p));
  if (!hit) throw new Error("no Chrome/Edge found (set CHROME)");
  return hit;
}
function findFfmpeg() {
  if (process.env.FFMPEG) return process.env.FFMPEG;
  try { execFileSync("ffmpeg", ["-version"], { stdio: "ignore" }); return "ffmpeg"; } catch {}
  return execFileSync("python", ["-c", "import imageio_ffmpeg;print(imageio_ffmpeg.get_ffmpeg_exe())"]).toString().trim();
}

// ── minimal CDP client ────────────────────────────────────────────────────────
async function cdp(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  let id = 0;
  const pending = new Map(), listeners = [];
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && pending.has(msg.id)) {
      const { res, rej } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? rej(new Error(msg.error.message)) : res(msg.result);
    } else if (msg.method) listeners.forEach((l) => l(msg));
  };
  return {
    send: (method, params = {}, sessionId) => new Promise((res, rej) => {
      const i = ++id;
      pending.set(i, { res, rej });
      ws.send(JSON.stringify({ id: i, method, params, sessionId }));
    }),
    on: (f) => listeners.push(f),
    close: () => ws.close(),
  };
}

const port = 9400 + Math.floor(Math.random() * 400);
const profile = mkdtempSync(join(tmpdir(), "nfd-promo-"));
const browser = spawn(findBrowser(), [
  "--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`,
  "--window-size=1920,1080", "--hide-scrollbars", "--mute-audio", "--force-device-scale-factor=1",
  "--no-first-run", "--no-default-browser-check", "about:blank",
], { stdio: "ignore" });

let version;
for (let i = 0; i < 100 && !version; i++) {
  try { version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json(); } catch { await new Promise((r) => setTimeout(r, 100)); }
}
if (!version) throw new Error("browser did not start");
const c = await cdp(version.webSocketDebuggerUrl);
const { targetId } = await c.send("Target.createTarget", { url: "about:blank" });
const { sessionId } = await c.send("Target.attachToTarget", { targetId, flatten: true });
const send = (m, p) => c.send(m, p, sessionId);
await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false });
const loaded = new Promise((r) => c.on((m) => m.method === "Page.loadEventFired" && r()));
await send("Page.navigate", { url: pathToFileURL(join(here, "index.html")).href + "?render=1" });
await loaded;
const ev = async (expr) => { const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true }); if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text); return r.result.value; };
await ev("document.fonts.ready.then(() => true)");
const total = await ev("TOTAL");
console.log(`timeline ${total.toFixed(2)} s`);

const shot = async (t, format = "jpeg") => {
  await ev(`seek(${t})`);
  const { data } = await send("Page.captureScreenshot", { format, quality: format === "jpeg" ? 94 : undefined, fromSurface: true });
  return Buffer.from(data, "base64");
};

try {
  if (cuesOut) {
    mkdirSync(dirname(resolve(cuesOut)), { recursive: true });
    writeFileSync(resolve(cuesOut), await ev("JSON.stringify(cues(), null, 1)"));
    console.log(resolve(cuesOut));
  } else if (stills) {
    const dir = join(root, "target", "promo", "stills");
    mkdirSync(dir, { recursive: true });
    for (const s of stills.split(",")) {
      const f = join(dir, `t${(+s).toFixed(2).padStart(7, "0")}.png`);
      writeFileSync(f, await shot(+s, "png"));
      console.log(f);
    }
  } else {
    mkdirSync(dirname(out), { recursive: true });
    const end = to ? +to : total;
    const n = Math.round((end - from) * fps);
    const ff = spawn(findFfmpeg(), [
      "-y", "-loglevel", "error", "-f", "image2pipe", "-framerate", String(fps), "-c:v", "mjpeg", "-i", "-",
      ...(audio ? ["-i", audio, "-c:a", "aac", "-b:a", "192k", "-shortest"] : []),
      "-vf", "scale=in_range=pc:out_range=tv,format=yuv420p", "-color_range", "tv", "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-movflags", "+faststart", out,
    ], { stdio: ["pipe", "inherit", "inherit"] });
    const done = new Promise((r, j) => ff.on("close", (code) => (code ? j(new Error("ffmpeg " + code)) : r())));
    const t0 = Date.now();
    for (let i = 0; i < n; i++) {
      const buf = await shot(from + i / fps);
      if (!ff.stdin.write(buf)) await new Promise((r) => ff.stdin.once("drain", r));
      if (i % (fps * 5) === 0) process.stdout.write(`\r${i}/${n} frames · ${((Date.now() - t0) / 1000).toFixed(0)} s`);
    }
    ff.stdin.end();
    await done;
    console.log(`\n${out}`);
  }
} finally {
  c.close();
  browser.kill();
  await new Promise((r) => setTimeout(r, 500));
  try { rmSync(profile, { recursive: true, force: true }); } catch {}
}
