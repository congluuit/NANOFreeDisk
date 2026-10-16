// node --test --experimental-strip-types src/lib/*.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";

import { cliArg, nfdScan, sizeFlag } from "./cli.ts";
import { daysBetween, formatDelta } from "./format.ts";
import { renderArticle } from "./md.ts";
import { fold, fuzzyScore, rank } from "./palette.ts";
import { buildReport, esc, treemapSvg } from "./report.ts";
import type { Detail, Slice, Summary } from "./types.ts";

test("cli arguments are quoted only when needed", () => {
  assert.equal(cliArg("C:\\"), "C:\\");
  assert.equal(cliArg("D:\\Data\\x.iso"), "D:\\Data\\x.iso");
  assert.equal(cliArg("C:\\Program Files\\"), '"C:\\Program Files\\\\"');
  assert.equal(cliArg("a b"), '"a b"');
  assert.equal(cliArg(""), '""');
  assert.equal(nfdScan("C:\\Users\\Công", "--json"), 'nfd scan "C:\\Users\\Công" --json');
  assert.equal(sizeFlag(2 ** 30), "1GB");
  assert.equal(sizeFlag(500 * 2 ** 20), "500MB");
});

test("palette matching folds case and Vietnamese diacritics", () => {
  assert.equal(fold("Cài đặt Đĩa"), "cai dat dia");
  assert.ok(fuzzyScore("cai dat", "Cài đặt") > 0);
  assert.equal(fuzzyScore("xyz", "Settings"), -1);
  assert.equal(fuzzyScore("", "anything"), 0);
  // prefix beats word start beats substring beats scattered letters
  const prefix = fuzzyScore("set", "Settings");
  const word = fuzzyScore("set", "Open settings");
  const inner = fuzzyScore("set", "Reset");
  const scattered = fuzzyScore("sts", "Settings");
  assert.ok(prefix > word && word > inner && inner > scattered && scattered > 0, `${prefix} ${word} ${inner} ${scattered}`);
  // words in any order
  assert.ok(fuzzyScore("dark theme", "Theme: Dark") > 0);
  assert.equal(fuzzyScore("dark zebra", "Theme: Dark"), -1);
  // widely scattered letters do not count
  assert.equal(fuzzyScore("cache", "Chuyển sang chế độ Chuyên gia"), -1);
  assert.equal(fuzzyScore("expert", "export html report"), -1);
});

test("palette ranking uses labels first, keywords second", () => {
  const items = [
    { label: "Analyze", keywords: "phân tích" },
    { label: "Settings", keywords: "cài đặt options" },
    { label: "Scan drive C:", keywords: "quét" },
  ];
  assert.deepEqual(rank(items, "").map((i) => i.label), ["Analyze", "Settings", "Scan drive C:"]);
  assert.deepEqual(rank(items, "s").map((i) => i.label)[0], "Settings");
  assert.deepEqual(rank(items, "cai dat").map((i) => i.label), ["Settings"]);
  assert.deepEqual(rank(items, "quet").map((i) => i.label), ["Scan drive C:"]);
  assert.equal(rank(items, "a", 1).length, 1);
});

const t = (id: string, args?: Record<string, string | number>) =>
  args ? `${id}(${Object.entries(args).map(([k, v]) => `${k}=${v}`).join(",")})` : id;

function slice(): Slice {
  return {
    ids: [0, 1, 2, 3, 4294967295],
    parents: [-1, 0, 1, 0, 0],
    sizes: [100, 60, 60, 30, 10],
    kinds: [1, 1, 0, 0, 3],
    names: ["C:\\", "Games", "big <x>.iso", "a.mp4", ""],
    depths: [0, 1, 2, 1, 1],
    truncated: true,
  };
}

test("report treemap escapes names and colors tiles with theme tokens", () => {
  const svg = treemapSvg(slice(), t, "en");
  assert.ok(svg.startsWith("<svg"));
  assert.ok(svg.includes("big &#60;x&#62;.iso"));
  assert.ok(!svg.includes("<x>"));
  assert.ok(svg.includes("var(--tm-dir)"));
  assert.ok(svg.includes("var(--tm-4)"), "iso is an archive");
  assert.ok(svg.includes("var(--tm-1)"), "mp4 is a video");
  assert.ok(svg.includes("var(--tm-rest)"));
  assert.ok(!/#[0-9a-f]{6}/i.test(svg), "no hard-coded colors");
  assert.equal(esc(`<a href="x">&'`), "&#60;a href=&#34;x&#34;&#62;&#38;&#39;");
});

test("report is one self-contained document", () => {
  const folder: Detail = {
    id: 0, name: "C:\\", kind: "dir", logical: 100, onDisk: 100, files: 2, mtime: 0, attrs: [], hasChildren: true,
    hardlinkDup: false, path: "C:\\", ancestors: [{ id: 0, name: "C:\\" }], share: 1,
  };
  const summary: Summary = {
    root: "C:\\", rootId: 0, elapsedMs: 10, scannedAt: 0, issues: 0, mode: "standard", volumeTotal: 1000, volumeFree: 900,
    stats: { files: 2, dirs: 1, symlinks: 0, junctions: 0, mount_points: 0, other: 0, hardlink_dups: 0, logical: 100, on_disk: 100, errors: 0, approximate: 0 },
  };
  const html = buildReport(
    {
      folder, summary, slice: slice(), children: [], extensions: [{ ext: "iso", files: 1, onDisk: 60, logical: 60 }],
      topFiles: [{ ...folder, id: 2, name: "big <x>.iso", kind: "file", path: "C:\\Games\\big <x>.iso" }],
      oldLarge: {
        large: { rows: [], matched: 1, matchedOnDisk: 60 }, stale: { rows: [], matched: 0, matchedOnDisk: 0 },
        installers: { rows: [], matched: 0, matchedOnDisk: 0 }, distinctFiles: 1, distinctOnDisk: 60,
      },
      vars: { bg: "#000000", "tm-1": "#ff00ff", "evil;}": "x", text: "red;}</style>" },
      dark: true, version: "9.9.9", generatedAt: 0, locale: "en-US",
    },
    t,
  );
  assert.ok(html.startsWith("<!doctype html>"));
  assert.ok(html.includes("--bg: #000000;"));
  assert.ok(!html.includes("evil") && !html.includes("red;}"), "unsafe CSS variables are dropped");
  assert.ok(!/<script|<link|src=|url\(/i.test(html), "no scripts or external resources");
  assert.ok(html.includes("report-footer(version=9.9.9)"));
  assert.ok(html.includes("analyze-truncated"));
  assert.ok(html.includes("ol-summary(files=1"));
  assert.equal((html.match(/<x>/g) ?? []).length, 0);
  assert.ok(!html.includes("report-changes"), "no history, no section");
});

test("report includes What changed? when a diff is given", () => {
  const snap = { id: 1, root: "C:\\", volume: "c:\\", startedAt: 0, elapsedMs: 0, files: 1, dirs: 1, onDisk: 1, volumeTotal: 1, volumeFree: 1, issues: 0, dirFloor: 0, fileFloor: 0 };
  const folder: Detail = {
    id: 0, name: "C:\\", kind: "dir", logical: 1, onDisk: 1, files: 1, mtime: 0, attrs: [], hasChildren: false,
    hardlinkDup: false, path: "C:\\", ancestors: [], share: 1,
  };
  const html = buildReport(
    {
      folder, slice: slice(), children: [], extensions: [], topFiles: [], oldLarge: null,
      summary: { root: "C:\\", rootId: 0, elapsedMs: 0, scannedAt: 0, issues: 0, mode: "standard", volumeTotal: 0, volumeFree: 0,
        stats: { files: 1, dirs: 1, symlinks: 0, junctions: 0, mount_points: 0, other: 0, hardlink_dups: 0, logical: 1, on_disk: 1, errors: 0, approximate: 0 } },
      changes: {
        older: snap, newer: { ...snap, id: 2, startedAt: 7 * 86_400 }, onDiskDelta: 2 ** 30, filesDelta: 3, freeDelta: -(2 ** 30),
        grew: [{ path: "Games\\<big>", depth: 1, before: 0, after: 2 ** 30, delta: 2 ** 30, filesDelta: 3 }], shrank: [], filesUp: [], filesDown: [],
      },
      vars: {}, dark: false, version: "1", generatedAt: 0, locale: "en-US",
    },
    t,
  );
  assert.ok(html.includes("report-changes(date="));
  assert.ok(html.includes("changes-summary(days=7,size=+1.0 GB,files=+3)"));
  assert.ok(html.includes("Games\\&#60;big&#62;"));
});

test("signed size changes and day counts", () => {
  assert.equal(formatDelta(1.5 * 2 ** 30), "+1.5 GB");
  assert.equal(formatDelta(-200 * 2 ** 20), "−200 MB");
  assert.equal(formatDelta(0), "0 B");
  assert.equal(daysBetween(0, 7 * 86_400 + 3600), 7);
  assert.equal(daysBetween(10, 0), 0);
});

test("encyclopedia markdown is escaped and minimal", () => {
  assert.equal(
    renderArticle("Line one\nwraps.\n\n**If you delete it:** run `a <b> **c**`.\n\n<script>x</script>"),
    "<p>Line one wraps.</p><p><strong>If you delete it:</strong> run <code>a &lt;b&gt; **c**</code>.</p><p>&lt;script&gt;x&lt;/script&gt;</p>",
  );
});
