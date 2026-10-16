#!/usr/bin/env node
// Side-by-side check of Hardware → Sensors against LibreHardwareMonitor (docs/HARDWARE_MONITOR.md §8).
// Start LibreHardwareMonitor elevated with its web server on 127.0.0.1:8085 (Options → Remote Web Server),
// make sure PawnIO and the IntelMSR module are present, then:
//   node scripts/hw-compare-lhm.mjs target/release/nanofreedisk.exe [rounds] [load]
// 'load' only waits 20 s for temperatures to settle; start the load yourself (see docs/hwmon-validation.md).
// The app asks for UAC once (Advanced sensors helper). Read-only on both sides.
import { spawn } from "node:child_process";
const exe = process.argv[2];
const ROUNDS = Number(process.argv[3] ?? 15);
const LOAD = process.argv[4] === "load";
const PORT = 9566;
const child = spawn(exe, [], { env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` }, stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let page;
for (let i = 0; i < 80 && !page; i++) {
  try { page = (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).find((t) => t.type === "page"); } catch {}
  await sleep(250);
}
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let seq = 0; const pending = new Map();
ws.onmessage = (m) => { const msg = JSON.parse(m.data); pending.get(msg.id)?.(msg.result); pending.delete(msg.id); };
const send = (method, params = {}) => new Promise((res) => { const id = ++seq; pending.set(id, res); ws.send(JSON.stringify({ id, method, params })); });
const ev = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result.value;
const inv = (c, a = {}) => ev(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(c)}, ${JSON.stringify(a)}).then(v => v, e => ({ __error: e }))`);

async function lhm() {
  const j = await (await fetch("http://127.0.0.1:8085/data.json")).json();
  const out = new Map();
  const walk = (n) => {
    if (n.SensorId && n.Value) {
      const v = parseFloat(String(n.Value).replace(",", "."));
      if (Number.isFinite(v)) out.set(n.SensorId, n.Value.includes("GHz") ? v * 1000 : v);
    }
    (n.Children ?? []).forEach(walk);
  };
  walk(j);
  return out;
}

const C = "/intelcpu/0";
const pairs = [
  ["CPU Package temp", "temp", `${C}/temperature/26`, "cpu/cpu-package"],
  ["Core Max temp", "temp", `${C}/temperature/0`, "cpu/cpu-core-max"],
  ["Core Average temp", "temp", `${C}/temperature/1`, "cpu/cpu-core-avg"],
  ...Array.from({ length: 24 }, (_, i) => [`Core #${i + 1} temp`, "temp", `${C}/temperature/${i + 2}`, `cpu/cpu-core-temp/${i + 1}`]),
  ["CPU Package power", "pct", `${C}/power/0`, "cpu/cpu-power-package"],
  ["CPU Cores power", "pct", `${C}/power/1`, "cpu/cpu-power-cores"],
  ["CPU Platform power", "abs0.5", `${C}/power/4`, "cpu/cpu-power-psys"],
  ...Array.from({ length: 24 }, (_, i) => [`Core #${i + 1} VID`, "pct", `${C}/voltage/${i + 1}`, `cpu/cpu-vid/${i + 1}`]),
  ...Array.from({ length: 24 }, (_, i) => [`Core #${i + 1} clock`, "pct", `${C}/clock/${i + 1}`, `cpu/cpu-clock-core/${i + 1}`]),
  ["Bus speed", "pct", `${C}/clock/0`, "cpu/cpu-bus"],
  ["CPU Total load", "abs5", `${C}/load/0`, "cpu/cpu-total"],
  ["Memory load", "abs2", "/ram/load/0", "memory/mem-load"],
  ["NVIDIA GPU temp", "temp", "/gpu-nvidia/0/temperature/0", "gpu1/gpu-temp"],
  ["NVIDIA GPU power", "pct", "/gpu-nvidia/0/power/0", "gpu1/gpu-power"],
  ["NVIDIA GPU core clock", "pct", "/gpu-nvidia/0/clock/0", "gpu1/gpu-clock-core"],
  ["NVIDIA GPU memory clock", "pct", "/gpu-nvidia/0/clock/4", "gpu1/gpu-clock-mem"],
  ["980 PRO temp", "temp", "/nvme/0/temperature/0", "disk0/disk-temp"],
  ["MZVLB512 temp", "temp", "/nvme/1/temperature/0", "disk1/disk-temp"],
];

try {
  for (let i = 0; i < 40 && !(await ev("!!window.__TAURI_INTERNALS__")); i++) await sleep(250);
  const st = await inv("advanced_sensors_start");
  if (!st.active) throw new Error("advanced sensors did not start: " + JSON.stringify(st));
  if (LOAD) {
    // the load runs in separate, affinity-pinned processes (see load.mjs)
    await sleep(20000); // let temperatures and power settle under PL1
  }
  await inv("sensors_snapshot");
  await sleep(1100);
  await inv("sensors_snapshot");
  const acc = new Map(pairs.map((p) => [p[0], { a: [], b: [], d: [] }]));
  for (let r = 0; r < ROUNDS; r++) {
    await sleep(1000);
    const [L, S] = await Promise.all([lhm(), inv("sensors_snapshot")]);
    const ours = new Map(S.sensors.map((s) => [s.id, s.value]));
    for (const [name, , lid, oid] of pairs) {
      const a = L.get(lid), b = ours.get(oid);
      if (a == null || b == null) continue;
      const x = acc.get(name);
      x.a.push(a); x.b.push(b); x.d.push(Math.abs(a - b));
    }
  }
  const mean = (v) => v.reduce((s, x) => s + x, 0) / v.length;
  let fails = 0;
  const rows = [];
  for (const [name, tol] of pairs) {
    const x = acc.get(name);
    if (!x.a.length) { rows.push([name, "-", "-", "-", "missing"]); fails++; continue; }
    const a = mean(x.a), b = mean(x.b), diff = b - a;
    const limit = tol === "temp" ? 2 : tol === "pct" ? Math.max(Math.abs(a) * 0.05, 1e-9) : Number(tol.slice(3));
    const ok = Math.abs(diff) <= limit;
    if (!ok) fails++;
    rows.push([name, a.toFixed(2), b.toFixed(2), (diff >= 0 ? "+" : "") + diff.toFixed(2), ok ? "ok" : `OUT (±${limit.toFixed(2)})`, mean(x.d).toFixed(2)]);
  }
  console.log(["sensor", "LHM", "NFD", "diff", "verdict", "mean |pair diff|"].join("\t"));
  for (const r of rows) console.log(r.join("\t"));
  console.log(`${ROUNDS} rounds, ${rows.length - fails}/${rows.length} within tolerance`);
} finally {
  ws.close(); child.kill(); await sleep(1500);
}
