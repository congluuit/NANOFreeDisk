import { test } from "node:test";
import assert from "node:assert/strict";

import { diskLabel, filesOf, hourBars, niceMax, parentDir, seriesPath, sortProcs } from "./monitor/view.ts";
import { monitorHandlers } from "./monitor/mock.ts";
import type { Minute, MonitorView, ProcRate } from "./monitor/types.ts";

const MB = 1024 * 1024;

const proc = (pid: number, name: string, read: number, write: number, ops = 0): ProcRate => ({
  pid,
  createTime: String(pid),
  name,
  path: null,
  read,
  write,
  readOps: ops,
  writeOps: ops,
  files: 0,
});

test("processes sort by any column, ties by PID", () => {
  const list = [proc(3, "b.exe", 10, 5), proc(1, "A.exe", 10, 50, 7), proc(2, "c.exe", 99, 5, 1)];
  assert.deepEqual(sortProcs(list, "write", true).map((p) => p.pid), [1, 2, 3]);
  assert.deepEqual(sortProcs(list, "read", true).map((p) => p.pid), [2, 1, 3]);
  assert.deepEqual(sortProcs(list, "name", false).map((p) => p.pid), [1, 3, 2]);
  assert.deepEqual(sortProcs(list, "iops", true).map((p) => p.pid), [1, 2, 3]);
  assert.equal(list[0].pid, 3, "input untouched");
});

test("files of a process, most written first", () => {
  const f = (pid: number, path: string, write: number) => ({ pid, process: "x", path, read: 0, write, ops: 1 });
  const out = filesOf([f(1, "a", 1), f(2, "b", 9), f(1, "c", 5)], 1);
  assert.deepEqual(out.map((x) => x.path), ["c", "a"]);
});

test("parent folder of a Windows path", () => {
  assert.equal(parentDir("C:\\logs\\app\\debug.log"), "C:\\logs\\app");
  assert.equal(parentDir("C:\\debug.log"), "C:\\");
  assert.equal(parentDir("nofolder"), "nofolder");
});

test("chart axis maximum is round and at least 1 MB/s", () => {
  assert.equal(niceMax(0), MB);
  assert.equal(niceMax(1.5 * MB), 2 * MB);
  assert.equal(niceMax(7 * MB), 10 * MB);
  assert.equal(niceMax(300 * MB), 500 * MB);
  assert.equal(niceMax(1500 * MB), 2 * 1024 * MB);
});

test("series path scales into the box and breaks at pauses", () => {
  const pts = [
    { t: 100, read: 0, write: 0 },
    { t: 101, read: MB, write: 0 },
    { t: 110, read: 2 * MB, write: 0 },
  ];
  const d = seriesPath(pts, "read", 110, 10, 100, 50, 2 * MB);
  assert.equal(d, "M0.0 50.0 L10.0 25.0 M100.0 0.0", "101 → 110 is a gap: a new segment");
  assert.equal(seriesPath(pts, "read", 110, 5, 100, 50, 2 * MB), "M100.0 0.0", "older points are outside the window");
  assert.equal(seriesPath([], "read", 0, 60, 100, 50, MB), "");
});

test("disk labels split the PDH instance", () => {
  const d = { name: "0 C: E:", read: 0, write: 0, transfers: 0, queue: 0, latencyMs: 0, idle: 100 };
  assert.deepEqual(diskLabel(d), { index: "0", letters: "C: E:" });
  assert.deepEqual(diskLabel({ ...d, name: "Odd" }), { index: "", letters: "Odd" });
});

test("24 hourly bars from saved minutes", () => {
  const now = 1_000_000 * 3600 + 1800;
  const m = (ts: number, write: number): Minute => ({ ts, read: 1, write, peakRead: 0, peakWrite: 0, seconds: 60, top: [] });
  const bars = hourBars([m(now - 60, 5), m(now - 120, 6), m(now - 3 * 3600, 7), m(now - 30 * 3600, 99)], now);
  assert.equal(bars.length, 24);
  assert.equal(bars[23].write, 11);
  assert.equal(bars[23].minutes, 2);
  assert.equal(bars[20].write, 7);
  assert.equal(bars.reduce((a, b) => a + b.write, 0), 18, "older than 24 h is left out");
});

test("mock backend answers like the real commands", () => {
  const v = monitorHandlers.monitor_snapshot({}) as MonitorView;
  assert.equal(v.snapshot?.schema, "nanofreedisk.monitor.v1");
  assert.equal(v.snapshot?.source, "counters");
  assert.equal(v.snapshot?.files.length, 0, "no files without detail");
  const d = monitorHandlers.monitor_detail({ enabled: true }) as MonitorView;
  assert.ok(d.detail.running && d.snapshot!.files.length > 0);
  monitorHandlers.monitor_pause({ paused: true });
  assert.equal((monitorHandlers.monitor_snapshot({}) as MonitorView).paused, true);
});
