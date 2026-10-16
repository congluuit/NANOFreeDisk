// node --test --experimental-strip-types src/lib/*.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";

import { healthHandlers } from "./health/mock.ts";
import { buildHealthReport } from "./health/report.ts";
import type { HealthReport } from "./health/types.ts";
import { forecast, heatClass, nvmeRows, reasonArgs, statusToken, volumeRows, worst } from "./health/view.ts";

const t = (id: string, args?: Record<string, string | number>) =>
  args ? `${id}(${Object.entries(args).map(([k, v]) => `${k}=${v}`).join(",")})` : id;

const report = () => healthHandlers.health_report({}) as HealthReport;

test("verdict tones are theme tokens", () => {
  assert.equal(statusToken("good"), "--risk-safe");
  assert.equal(statusToken("caution"), "--risk-caution");
  assert.equal(statusToken("bad"), "--risk-protected");
  assert.equal(statusToken("unknown"), "--text-muted");
  assert.equal(heatClass("critical"), "hot");
  assert.equal(heatClass("warning"), "warm");
  assert.equal(heatClass("unknown"), "");
});

test("reasons name the attribute", () => {
  const r = report();
  const hdd = r.drives[1];
  assert.deepEqual(reasonArgs(hdd.reasons[0], hdd), { value: 8, attr: "C5", name: "Current Pending Sector Count" });
  assert.equal(worst(r), "caution");
  assert.equal(worst({ ...r, drives: [] }), "unknown");
});

test("Simple mode shows the key NVMe fields, Expert all of them", () => {
  const nvme = report().drives[0];
  const simple = nvmeRows(nvme, t, "en", false);
  const full = nvmeRows(nvme, t, "en", true);
  assert.equal(simple.length, 5);
  assert.ok(full.length > simple.length);
  assert.equal(simple[0][1], "health-none");
  assert.ok(simple.some(([k, v]) => k === "health-nvme-used" && v === "4 %"));
  assert.deepEqual(nvmeRows(report().drives[1], t, "en", true), []);
});

test("volume checks flag only real problems", () => {
  const r = report();
  const c = r.volumes[0];
  assert.equal(forecast(c, t), "health-vol-full-in(days=41)");
  assert.equal(forecast({ ...c, daysToFull: null }, t), "health-vol-not-filling");
  assert.equal(forecast({ ...c, daysToFull: null, hasTrend: false }, t), "health-vol-no-trend");
  const problems = (v = c, d = r.drives[0]) => volumeRows(v, d, t, "en").filter((x) => x[2]).map((x) => x[0]);
  assert.deepEqual(problems(), []);
  assert.deepEqual(problems({ ...c, free: c.total * 0.05, daysToFull: 10, dirty: true }), ["health-vol-free", "health-vol-forecast", "health-vol-dirty"]);
  // TRIM off only matters on an SSD that supports it
  assert.deepEqual(problems({ ...c, trimEnabled: false }), ["health-vol-trim"]);
  assert.deepEqual(problems({ ...c, trimEnabled: false }, r.drives[1]), []);
  assert.ok(volumeRows({ ...c, dirty: null }, r.drives[0], t, "en").some(([, v]) => v === "health-not-checked"));
});

test("health report is self-contained and hides serials by default", () => {
  const input = { report: report(), vars: { bg: "#000000", "x;}": "y", text: "red;}</style>" }, dark: true, version: "9.9.9", locale: "en-US" };
  const html = buildHealthReport({ ...input, serials: false }, t);
  assert.ok(html.startsWith("<!doctype html>"));
  assert.ok(html.includes("--bg: #000000;"));
  assert.ok(!html.includes("red;}"), "unsafe CSS variables are dropped");
  assert.ok(!/<script|<link|src=|url\(/i.test(html), "no scripts or external resources");
  assert.ok(html.includes("Samsung SSD 980 PRO 1TB") && html.includes("WDC WD20EZRZ"));
  assert.ok(html.includes("health-reason-ata-pending(value=8,attr=C5"));
  assert.ok(html.includes("health-note-usb-bridge"));
  assert.ok(html.includes("var(--risk-caution)"));
  assert.ok(!html.includes("S5P2NS0W") && !html.includes("WD-WCC4M"), "no serial numbers");
  const withSerials = buildHealthReport({ ...input, serials: true }, t);
  assert.ok(withSerials.includes("S5P2NS0W000000X"));
  // names are escaped
  const r = report();
  r.drives[0].model = "<b>evil</b>";
  assert.ok(!buildHealthReport({ ...input, report: r, serials: false }, t).includes("<b>evil"));
});
