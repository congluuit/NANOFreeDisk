// Duplicates: the size limit (KB or MB, the saved setting, the old 1 MB default),
// the "Not compared" lines with what each one offers, the one-click search of the
// browser mock, and a message for every reason and action in both languages.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { DEFAULT_MIN_BYTES, cliSize, lowerMinSize, minBytesFromSetting, settingFromBytes, splitSize, toBytes } from "./dupes/size.ts";
import { skipLines } from "./dupes/skipped.ts";
import { dupeHandlers } from "./dupes/mock.ts";
import type { DupeReport, SkipReason } from "./dupes/types.ts";

const KIB = 1024;
const MIB = 1024 * 1024;

test("the default limit is 100 KiB", () => {
  assert.equal(DEFAULT_MIN_BYTES, 100 * KIB);
  assert.equal(minBytesFromSetting(0), DEFAULT_MIN_BYTES);
  assert.equal(minBytesFromSetting(Number.NaN), DEFAULT_MIN_BYTES);
  assert.equal(minBytesFromSetting(5 * 1024), 5 * MIB);
  assert.equal(minBytesFromSetting(512), 512 * KIB);
});

test("a limit chosen by hand survives the saved setting, 1 MB included", () => {
  for (const bytes of [1, 10 * KIB, 100 * KIB, 250 * KIB, MIB, 3 * MIB, 1536 * KIB]) {
    assert.equal(minBytesFromSetting(settingFromBytes(bytes)), bytes, String(bytes));
  }
});

test("the field shows KB below 1 MB and MB from there", () => {
  assert.deepEqual(splitSize(100 * KIB), { value: 100, unit: "KB" });
  assert.deepEqual(splitSize(MIB), { value: 1, unit: "MB" });
  assert.deepEqual(splitSize(1536 * KIB), { value: 1.5, unit: "MB" });
  assert.equal(toBytes(100, "KB"), 100 * KIB);
  assert.equal(toBytes(2.5, "MB"), 2.5 * MIB);
  // empty files never count: the limit is at least one byte
  assert.equal(toBytes(0, "KB"), 1);
  assert.equal(toBytes(-3, "MB"), 1);
  assert.equal(cliSize(100 * KIB), "100KB");
  assert.equal(cliSize(MIB), "1MB");
});

test("include smaller files steps down to 10 KB and stops", () => {
  assert.equal(lowerMinSize(5 * MIB), MIB);
  assert.equal(lowerMinSize(MIB), 100 * KIB);
  assert.equal(lowerMinSize(100 * KIB), 10 * KIB);
  assert.equal(lowerMinSize(10 * KIB), null);
  assert.equal(lowerMinSize(1), null);
});

test("not compared: one line per reason, actions where something can be done", () => {
  const lines = skipLines({
    minSize: 100 * KIB,
    skipped: { small: 900, cloud: 3, "app-dir": 40, changed: 1, protected: 2, empty: 7, extension: 5 },
    skippedSamples: [
      { path: "C:\\a\\x.jpg", reason: "cloud" },
      { path: "C:\\a\\y.jpg", reason: "cloud" },
      { path: "C:\\a\\z.jpg", reason: "cloud" },
      { path: "C:\\a\\w.jpg", reason: "cloud" },
      { path: "C:\\p\\node_modules\\m.js", reason: "app-dir" },
    ],
  });
  assert.deepEqual(
    lines.map((l) => l.reason),
    ["small", "app-dir", "extension", "cloud", "protected", "changed", "empty"],
  );
  const by = Object.fromEntries(lines.map((l) => [l.reason, l]));
  assert.deepEqual(by.small.action, { kind: "lower", bytes: 10 * KIB });
  assert.deepEqual(by["app-dir"].action, { kind: "app-dirs" });
  assert.deepEqual(by.extension.action, { kind: "all-types" });
  assert.deepEqual(by.changed.action, { kind: "again" });
  assert.equal(by.cloud.action, null);
  assert.equal(by.protected.action, null);
  assert.equal(by.empty.action, null);
  assert.equal(by.cloud.samples.length, 3);
  assert.deepEqual(by["app-dir"].samples, ["C:\\p\\node_modules\\m.js"]);
  // nothing lower than 10 KB is offered
  assert.equal(skipLines({ minSize: 10 * KIB, skipped: { small: 4 }, skippedSamples: [] })[0].action, null);
});

test("the mock: one click searches the personal folders, options shape the report", () => {
  const base = { includeAppDirs: false, paranoid: false, extensions: [], useCache: false, folders: false };
  const mine = dupeHandlers.dupes_find({ paths: [], options: { ...base, minSize: 100 * KIB, personal: true } }) as DupeReport;
  assert.ok(mine.roots.length >= 5 && mine.roots.some((r) => r.endsWith("\\Downloads")), mine.roots.join());
  assert.equal(mine.minSize, 100 * KIB);
  assert.ok((mine.skipped["app-dir"] ?? 0) > 0);
  const withApps = dupeHandlers.dupes_find({ paths: ["D:\\"], options: { ...base, minSize: 10 * KIB, includeAppDirs: true } }) as DupeReport;
  assert.deepEqual(withApps.roots, ["D:\\"]);
  assert.equal(withApps.skipped["app-dir"], undefined);
  assert.equal(withApps.minSize, 10 * KIB);
});

function keys(lang: string): Set<string> {
  const text = readFileSync(new URL(`../../../../locales/${lang}/dupes.ftl`, import.meta.url), "utf8");
  return new Set([...text.matchAll(/^([a-z][\w-]*) =/gm)].map((m) => m[1]));
}

test("every reason, action and keep rule has its text in both languages", () => {
  const reasons: SkipReason[] = ["empty", "small", "extension", "cloud", "reparse", "system", "protected", "app-dir", "unreadable", "changed", "clone"];
  const wanted = [
    ...reasons.flatMap((r) => [`dupes-skip-${r}`, `dupes-skip-${r}-why`]),
    ...["lower", "app-dirs", "all-types", "again"].flatMap((a) => [`dupes-skip-act-${a}`, `dupes-skip-act-${a}-hint`]),
    ...["auto", "oldest", "newest", "shortest"].map((s) => `dupes-preselect-${s}`),
    "dupes-not-compared",
    "dupes-headline",
    "dupes-headline-none",
    "dupes-find-mine",
    "dupes-choose",
    "dupes-options",
    "dupes-keeps",
  ];
  for (const lang of ["en", "vi"]) {
    const have = keys(lang);
    assert.deepEqual(wanted.filter((k) => !have.has(k)), [], lang);
  }
});
