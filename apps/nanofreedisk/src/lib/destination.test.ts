// One destination in Simple mode (docs/PHASE3_CLEAN.md, "Simple mode: one
// destination"): every tool sends removed items to the quarantine, Expert mode keeps
// each tool's own default, big removals may choose the Recycle Bin, and the shared
// confirmation / result keys exist in both languages without long dashes.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

import {
  BIG_REMOVAL_BYTES,
  EXPERT_DEFAULT,
  REMOVAL_KEYS,
  destinationFor,
  isBigRemoval,
  lowDrives,
  outcomesResult,
  removeKeys,
  safetyNoteKey,
  volumeKey,
  volumeOf,
  type RemovalTool,
} from "./clean/destination.ts";
import type { Volume } from "./types.ts";

const GB = 2 ** 30;
const vol = (root: string, total: number, available: number): Volume => ({ root, label: "", fs: "NTFS", kind: "fixed", total, available, free: available, ready: true, system: root.startsWith("C") });
const VOLUMES = [vol("C:\\", 500 * GB, 200 * GB), vol("D:\\", 1000 * GB, 50 * GB)];

const LOCALES = new URL("../../../../locales/", import.meta.url);
/** The locale files of the cleaning tools (other files belong to other features). */
const FILES = ["app.ftl", "clean.ftl", "dupes.ftl", "devcache.ftl", "selection.ftl", "advice.ftl", "browsers.ftl", "ai-leftovers.ftl", "programs.ftl", "ui.ftl", "recyclebin.ftl"];
function messages(lang: string): Map<string, string> {
  const out = new Map<string, string>();
  for (const file of readdirSync(new URL(`${lang}/`, LOCALES))) {
    if (!FILES.includes(file)) continue;
    for (const line of readFileSync(new URL(`${lang}/${file}`, LOCALES), "utf8").split(/\r?\n/)) {
      const m = /^([a-z][\w-]*)\s*=\s*(.*)$/.exec(line);
      if (m) out.set(m[1], m[2]);
    }
  }
  return out;
}

test("Simple mode: one destination for every tool", () => {
  for (const tool of ["clean", "tidy", "manual", "dupes"] as RemovalTool[]) {
    assert.equal(destinationFor(tool, false), "quarantine", tool);
  }
});

test("Expert mode keeps each tool's own default", () => {
  assert.equal(destinationFor("clean", true), "quarantine");
  assert.equal(destinationFor("tidy", true), "quarantine");
  assert.equal(destinationFor("manual", true), "recycle-bin");
  assert.equal(destinationFor("dupes", true), "recycle-bin");
  for (const [tool, dest] of Object.entries(EXPERT_DEFAULT)) assert.equal(destinationFor(tool as RemovalTool, true), dest);
});

test("drives of a path", () => {
  assert.equal(volumeKey("C:\\Users\\a"), "c:");
  assert.equal(volumeKey("\\\\?\\D:\\x"), "d:");
  assert.equal(volumeKey("\\\\server\\share\\x"), "//server/share");
  assert.equal(volumeKey("relative\\x"), null);
  assert.equal(volumeOf("d:\\big.iso", VOLUMES)?.root, "D:\\");
  assert.equal(volumeOf("E:\\x", VOLUMES), null);
  const mac = [vol("/", 500 * GB, 100 * GB), vol("/Volumes/USB", 64 * GB, 1 * GB)];
  assert.equal(volumeOf("/Volumes/USB/a.mov", mac)?.root, "/Volumes/USB");
  assert.equal(volumeOf("/Users/a/b", mac)?.root, "/");
});

test("a big removal, or one from an almost full drive, may choose the Recycle Bin", () => {
  assert.equal(isBigRemoval(BIG_REMOVAL_BYTES - 1, ["C:\\a"], VOLUMES), false);
  assert.equal(isBigRemoval(BIG_REMOVAL_BYTES, ["C:\\a"], VOLUMES), true);
  // D: has 5 % free
  assert.equal(isBigRemoval(1024, ["D:\\a"], VOLUMES), true);
  assert.deepEqual(lowDrives(["D:\\a", "d:\\b", "C:\\c"], VOLUMES).map((v) => v.root), ["D:\\"]);
  // unknown drives never count as almost full
  assert.equal(isBigRemoval(1024, ["Z:\\a"], VOLUMES), false);
});

test("button labels follow the destination", () => {
  assert.equal(removeKeys("quarantine").bar, "sel-remove-n");
  assert.equal(removeKeys("recycle-bin").bar, "sel-recycle-n");
  assert.equal(safetyNoteKey("quarantine"), "safety-note");
  assert.equal(safetyNoteKey("recycle-bin"), "safety-note-recycle");
  assert.equal(safetyNoteKey("permanent"), "safety-note-permanent");
});

test("a hand-picked delete becomes the shared result", () => {
  const r = outcomesResult(
    "op",
    [
      { id: 1, status: "done", reason: null, message: null, onDisk: 100 },
      { id: 2, status: "blocked", reason: "in-use", message: null, onDisk: 50 },
      { id: 3, status: "declined", reason: null, message: null, onDisk: 7 },
      { id: 4, status: "failed", reason: null, message: "x", onDisk: 9 },
    ],
    new Map([
      [1, "C:\\a"],
      [2, "C:\\b"],
      [3, "C:\\c"],
    ]),
  );
  assert.equal(r.freed, 100);
  assert.equal(r.done, 1);
  assert.equal(r.skipped, 2);
  assert.equal(r.failed, 1);
  assert.deepEqual(
    r.problems.map((p) => [p.path, p.status, p.reason]),
    [
      ["C:\\b", "skipped", "in-use"],
      ["C:\\c", "declined", null],
      ["4", "failed", null],
    ],
  );
});

test("the shared confirmation and result speak both languages, with one sentence about the quarantine", () => {
  const en = messages("en");
  const vi = messages("vi");
  for (const key of [...REMOVAL_KEYS, "clean-status-declined", "clean-left-title", "clean-dismiss", "delete-title-quarantine", "delete-confirm-quarantine"]) {
    assert.ok(en.has(key), `en ${key}`);
    assert.ok(vi.has(key), `vi ${key}`);
  }
  // one name for Cleanup history in the strings of the cleaning tools
  for (const [key, text] of en) {
    assert.ok(!/\b(in|from|see) Activity\b|Activity log/.test(text), `en ${key}: ${text}`);
  }
  for (const [key, text] of vi) {
    assert.ok(!/(trong|từ|vào) Hoạt động(?! ổ)|Nhật ký dọn dẹp/.test(text), `vi ${key}: ${text}`);
  }
});

test("no long dashes in the strings of the cleaning tools", () => {
  for (const lang of ["en", "vi"]) {
    for (const file of FILES) {
      const text = readFileSync(new URL(`${lang}/${file}`, LOCALES), "utf8");
      for (const [i, line] of text.split(/\r?\n/).entries()) assert.ok(!/[\u2013\u2014]| -- /.test(line), `${lang}/${file}:${i + 1}: ${line}`);
    }
  }
});
