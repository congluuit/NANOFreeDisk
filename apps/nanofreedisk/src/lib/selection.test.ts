// Shared selection model (selection.ts) and the per-website browser panel helpers
// (browsers/sites.ts): what may be ticked, honest totals, per-level and per-group
// selection, and the messages both need in EN and VI.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  groupState,
  levelsPresent,
  needsAck,
  sanitize,
  selectAll,
  selectLevel,
  selectable,
  setGroup,
  summarize,
  toggle,
  type SelItem,
} from "./selection.ts";
import { siteItems, siteMatches, targetsOf } from "./browsers/sites.ts";
import type { SitesReport } from "./browsers/types.ts";

const items: SelItem[] = [
  { id: "a", bytes: 100, files: 2, level: "safe", group: "p1" },
  { id: "b", bytes: 50, files: 1, level: "safe", group: "p1", skip: "claude.exe" },
  { id: "c", bytes: 400, files: 7, level: "caution", group: "p1" },
  { id: "d", bytes: 900, files: 9, level: "advanced", group: "p2" },
  { id: "e", bytes: 1000, files: 1, level: "protected", group: "p2" },
  { id: "f", bytes: 30, files: 1, level: "keep", group: "p2" },
  { id: "g", bytes: 70, files: 3, level: "safe", group: "p2", locked: true },
];

test("Protected, Keep and locked items are never selectable; Advanced only in Expert", () => {
  assert.deepEqual(selectAll(items, false), ["a", "b", "c"]);
  assert.deepEqual(selectAll(items, true), ["a", "b", "c", "d"]);
  for (const id of ["e", "f", "g"]) {
    assert.equal(selectable(items.find((i) => i.id === id)!, true), false, id);
    assert.deepEqual(toggle(items, [], id, true), [], id);
  }
  assert.deepEqual(sanitize(items, ["a", "e", "zz", "d", "a"], false), ["a"]);
});

test("select by level never mixes in other levels", () => {
  assert.deepEqual(selectLevel(items, "safe", true), ["a", "b"]);
  assert.deepEqual(selectLevel(items, "caution", true), ["c"]);
  assert.deepEqual(selectLevel(items, "advanced", false), []);
  assert.deepEqual(levelsPresent(items, false), ["safe", "caution"]);
  assert.deepEqual(levelsPresent(items, true), ["safe", "caution", "advanced"]);
});

test("totals count likely-skipped items apart so 'will be freed' is honest", () => {
  const s = summarize(items, ["a", "b", "c", "e", "missing"], false);
  assert.equal(s.count, 3);
  assert.equal(s.files, 10);
  assert.equal(s.bytes, 500);
  assert.equal(s.skipCount, 1);
  assert.equal(s.skipBytes, 50);
  assert.deepEqual(s.skipReasons, ["claude.exe"]);
  assert.deepEqual(
    s.byLevel.map((l) => [l.level, l.count, l.bytes]),
    [
      ["safe", 2, 150],
      ["caution", 1, 400],
    ],
  );
  assert.equal(needsAck(s), true);
  assert.equal(needsAck(summarize(items, ["a"], false)), false);
  const none = summarize(items, [], true);
  assert.equal(none.count + none.bytes + none.skipCount, 0);
});

test("group checkbox ticks only what may be ticked in that group", () => {
  assert.equal(groupState(items, "p1", [], false), "none");
  const on = setGroup(items, "p1", ["d"], true, false);
  assert.deepEqual(on.sort(), ["a", "b", "c", "d"]);
  assert.equal(groupState(items, "p1", on, false), "all");
  assert.equal(groupState(items, "p1", ["a"], false), "some");
  assert.deepEqual(setGroup(items, "p1", on, false, false), ["d"]);
  // p2 holds only Protected / Keep / locked items in Simple mode
  assert.equal(groupState(items, "p2", [], false), "empty");
  assert.deepEqual(setGroup(items, "p2", [], true, true), ["d"]);
});

test("duplicate groups: the copy that stays is locked, so select all keeps one copy", () => {
  const copies: SelItem[] = [
    { id: "1|C:/a.jpg", bytes: 10, group: "1", level: "safe", locked: true },
    { id: "1|D:/a.jpg", bytes: 10, group: "1", level: "safe" },
    { id: "1|E:/a.jpg", bytes: 10, group: "1", level: "caution" },
  ];
  const all = selectAll(copies, false);
  assert.equal(all.length, 2);
  assert.ok(!all.includes("1|C:/a.jpg"));
  assert.deepEqual(setGroup(copies, "1", [], true, false).length, 2);
});

const report: SitesReport = {
  truncated: false,
  browsers: [
    {
      id: "chrome",
      name: "Chrome",
      ruleId: "browser.chrome.cache",
      exe: "chrome.exe",
      running: false,
      profiles: [
        {
          name: "Default",
          path: "C:\\U\\Chrome\\User Data\\Default",
          stores: [],
          sites: [
            { site: "youtube.com", bytes: 900, entries: 9, cleanableBytes: 400, cleanableEntries: 3, listedBytes: 500, stores: ["http-cache", "code-cache-js"] },
            { site: "example.com", bytes: 70, entries: 1, cleanableBytes: 0, cleanableEntries: 0, listedBytes: 70, stores: ["http-cache"] },
          ],
        },
      ],
    },
    {
      id: "edge",
      name: "Edge",
      ruleId: "browser.edge.cache",
      exe: "msedge.exe",
      running: true,
      profiles: [{ name: "Default", path: "C:\\U\\Edge\\Default", stores: [], sites: [{ site: "bing.com", bytes: 10, entries: 1, cleanableBytes: 10, cleanableEntries: 1, listedBytes: 0, stores: ["code-cache-js"] }] }],
    },
  ],
};

test("browser sites: only per-site removable bytes count; listed-only sites are locked; a running browser is skipped", () => {
  const it = siteItems(report);
  assert.equal(it.length, 3);
  const yt = it.find((i) => i.id.endsWith("|youtube.com"))!;
  assert.equal(yt.bytes, 400);
  assert.equal(it.find((i) => i.id.endsWith("|example.com"))!.locked, true);
  assert.equal(it.find((i) => i.id.endsWith("|bing.com"))!.skip, "msedge.exe");
  const s = summarize(it, selectAll(it, false), false);
  assert.equal(s.count, 2);
  assert.equal(s.bytes, 400);
  assert.equal(s.skipBytes, 10);
  assert.deepEqual(siteItems(report, "tube").map((i) => i.id), [yt.id]);
  assert.ok(siteMatches("mail.google.com", "google mail"));
  assert.ok(!siteMatches("youtube.com", "bing"));
  assert.deepEqual(targetsOf([yt.id, "garbage"]), [{ browser: "chrome", profile: "C:\\U\\Chrome\\User Data\\Default", site: "youtube.com", subdomains: false }]);
});

function keys(lang: string, files: string[]): Set<string> {
  const out = new Set<string>();
  for (const file of files) {
    const text = readFileSync(new URL(`../../../../locales/${lang}/${file}`, import.meta.url), "utf8");
    for (const line of text.split(/\r?\n/)) {
      const m = /^([a-z][\w-]*)\s*=/.exec(line);
      if (m) out.add(m[1]);
    }
  }
  return out;
}

test("every dynamic selection / browser message exists in both languages", () => {
  for (const lang of ["en", "vi"]) {
    const k = keys(lang, ["selection.ftl", "browsers.ftl", "clean.ftl", "app.ftl"]);
    for (const l of ["safe", "caution", "advanced"]) assert.ok(k.has(`clean-level-${l}`), `${lang}: clean-level-${l}`);
    for (const s of ["http-cache", "code-cache-js", "code-cache-wasm", "cache-storage", "firefox-cache"]) assert.ok(k.has(`br-store-${s}`), `${lang}: br-store-${s}`);
    for (const id of ["activity-kind-browser-site", "reason-app-running", "sel-summary", "sel-skip", "sel-ack"]) assert.ok(k.has(id), `${lang}: ${id}`);
  }
});
