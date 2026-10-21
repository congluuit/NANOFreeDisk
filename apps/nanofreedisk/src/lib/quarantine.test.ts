// Activity log → quarantine: which items it holds, the days left before they go by
// themselves, deleting them for good through the browser mock (per item, per
// operation, everything), and a message for every new text in both languages.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { daysLeft, heldItem, restorableItem } from "./clean/quarantine.ts";
import { cleanHandlers } from "./clean/mock.ts";
import type { ActivityDetail, OperationItem, OperationRow, PurgeResult, QuarantineInfo } from "./clean/types.ts";

function keys(lang: string): Set<string> {
  const out = new Set<string>();
  for (const file of ["clean.ftl", "ui.ftl", "app.ftl"]) {
    const text = readFileSync(new URL(`../../../../locales/${lang}/${file}`, import.meta.url), "utf8");
    for (const line of text.split(/\r?\n/)) {
      const m = /^([a-z][\w-]*)\s*=/.exec(line);
      if (m) out.add(m[1]);
    }
  }
  return out;
}

const base: OperationItem = {
  n: 0,
  path: "C:\\x",
  ruleId: null,
  mode: "quarantine",
  isDir: true,
  bytes: 10,
  files: 1,
  freed: 10,
  status: "done",
  reason: null,
  stored: "C:\\q\\op\\0\\x",
  restored: false,
  message: null,
  purged: false,
};

test("what the quarantine holds", () => {
  assert.ok(heldItem(base, "active"));
  assert.ok(heldItem({ ...base, mode: "registry", stored: "C:\\b\\op\\1.reg" }, "active"));
  assert.ok(heldItem({ ...base, status: "partial" }, "partly-restored"));
  // the Recycle Bin, gone, put back or never moved: not held
  assert.ok(!heldItem({ ...base, mode: "recycle-bin", stored: null }, "active"));
  assert.ok(restorableItem({ ...base, mode: "recycle-bin", stored: null }, "active"));
  assert.ok(!heldItem({ ...base, purged: true }, "active"));
  assert.ok(!restorableItem({ ...base, purged: true }, "active"));
  assert.ok(!heldItem({ ...base, restored: true }, "active"));
  assert.ok(!heldItem(base, "purged"));
  assert.ok(!heldItem({ ...base, mode: "permanent" }, "active"));
  assert.ok(!heldItem({ ...base, status: "failed" }, "active"));
});

test("days left before deleting by itself", () => {
  const now = 1_800_000_000;
  assert.equal(daysLeft(now + 7 * 86_400, now), 7);
  assert.equal(daysLeft(now + 3600, now), 1);
  assert.equal(daysLeft(now, now), 0);
  assert.equal(daysLeft(now - 86_400, now), 0);
});

test("delete for good: one item, one operation, everything", () => {
  const list = () => cleanHandlers.activity_list({ limit: 50 }) as OperationRow[];
  const info = () => cleanHandlers.quarantine_summary({}) as QuarantineInfo;
  const start = info();
  assert.ok(start.items >= 5, JSON.stringify(start));
  assert.ok(start.nextExpiry !== null && start.roots.length === 1);

  const clean = list().find((o) => o.kind === "clean" && o.quarantined === 3)!;
  assert.ok(clean.expiresAt! > clean.startedAt);
  const one = cleanHandlers.activity_purge({ id: clean.id, items: [1] }) as PurgeResult;
  assert.equal(one.items, 1);
  const after = list().find((o) => o.id === clean.id)!;
  assert.equal(after.quarantined, 2);
  assert.equal(after.state, "active");
  const items = (cleanHandlers.activity_items({ id: clean.id }) as ActivityDetail).items;
  assert.ok(items[1].purged && !items[0].purged);

  // an uninstall keeps its Recycle Bin item when its quarantine is deleted
  const un = list().find((o) => o.kind === "uninstall")!;
  assert.equal(un.restorable, 3);
  assert.equal(un.quarantined, 2);
  const all = cleanHandlers.activity_purge({ id: un.id, items: null }) as PurgeResult;
  assert.equal(all.items, 2);
  const unAfter = list().find((o) => o.id === un.id)!;
  assert.deepEqual([unAfter.quarantined, unAfter.restorable, unAfter.state], [0, 1, "active"]);

  const emptied = cleanHandlers.quarantine_empty({}) as PurgeResult;
  assert.equal(emptied.items, 2);
  assert.equal(info().items, 0);
  assert.equal(list().find((o) => o.id === clean.id)!.state, "purged");
});

test("every quarantine text exists in both languages", () => {
  const needed = [
    ...["title", "intro", "held", "items", "next", "none", "recycle-note", "open", "open-hint", "empty", "empty-hint", "empty-title", "empty-body", "only"].map(
      (k) => `quarantine-${k}`,
    ),
    ...["purge", "purge-hint", "purge-one", "purge-title", "purge-body", "purge-one-body", "purged", "purge-failed", "in-quarantine", "expires", "item-purged"].map(
      (k) => `activity-${k}`,
    ),
    ...["quarantine", "recycle-bin", "permanent"].map((m) => `activity-mode-${m}`),
  ];
  for (const lang of ["en", "vi"]) {
    const have = keys(lang);
    const missing = needed.filter((k) => !have.has(k));
    assert.deepEqual(missing, [], `${lang}: ${missing.join(", ")}`);
  }
});
