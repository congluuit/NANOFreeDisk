// Startup view (docs/STARTUP.md) and the Recycle Bin card: list filtering and
// sorting, what an entry allows, the browser mocks, and a message for every
// dynamic key the views build.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { canChange, needsConfirm, publisherOf, visibleEntries } from "./startup/model.ts";
import { resetStartupMock, startupHandlers } from "./startup/mock.ts";
import { recycleBinHandlers } from "./recyclebin/mock.ts";
import type { StartupEntry } from "./startup/types.ts";

function keys(lang: string): Set<string> {
  const out = new Set<string>();
  for (const file of ["startup.ftl", "recyclebin.ftl"]) {
    const text = readFileSync(new URL(`../../../../locales/${lang}/${file}`, import.meta.url), "utf8");
    for (const line of text.split(/\r?\n/)) {
      const m = /^([a-z][\w-]*)\s*=/.exec(line);
      if (m) out.add(m[1]);
    }
  }
  return out;
}

function entry(name: string, extra: Partial<StartupEntry> = {}): StartupEntry {
  return {
    id: `user-run|${name}`,
    name,
    command: `C:\\Apps\\${name}.exe`,
    exe: `C:\\Apps\\${name}.exe`,
    missing: false,
    publisher: null,
    origin: "user-run",
    scope: "user",
    location: "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
    enabled: true,
    disabledAt: null,
    caution: null,
    approvedName: name,
    ...extra,
  };
}

test("filters, search and sorting", () => {
  const list = [
    entry("zeta", { enabled: false }),
    entry("Alpha", { publisher: "Acme Corp" }),
    entry("beta", { scope: "machine", origin: "machine-run" }),
  ];
  assert.deepEqual(visibleEntries(list, "", "all", "name").map((e) => e.name), ["Alpha", "beta", "zeta"]);
  assert.deepEqual(visibleEntries(list, "", "disabled", "name").map((e) => e.name), ["zeta"]);
  assert.deepEqual(visibleEntries(list, "", "enabled", "name").map((e) => e.name), ["Alpha", "beta"]);
  assert.deepEqual(visibleEntries(list, "", "machine", "name").map((e) => e.name), ["beta"]);
  assert.deepEqual(visibleEntries(list, "", "user", "name").map((e) => e.name), ["Alpha", "zeta"]);
  // every word must match, in any field
  assert.deepEqual(visibleEntries(list, "acme alpha", "all", "name").map((e) => e.name), ["Alpha"]);
  assert.deepEqual(visibleEntries(list, "apps zeta", "all", "name").map((e) => e.name), ["zeta"]);
  // enabled first, then by name
  const status = visibleEntries([...list, entry("aaa", { enabled: false })], "", "all", "status").map((e) => e.name);
  assert.deepEqual(status, ["Alpha", "beta", "aaa", "zeta"]);
  // the input is not reordered
  assert.equal(list[0].name, "zeta");
});

test("what an entry allows", () => {
  assert.equal(canChange(entry("a")), true);
  assert.equal(canChange(entry("a", { scope: "machine" })), false);
  assert.equal(needsConfirm(entry("a")), false);
  assert.equal(needsConfirm(entry("a", { caution: "windows" })), true);
  assert.equal(needsConfirm(entry("a", { caution: "security" })), true);
  assert.equal(publisherOf(entry("a")), null);
  assert.deepEqual(publisherOf(entry("a", { publisher: "Acme Corp" })), { name: "Acme Corp", verified: true });
  assert.deepEqual(publisherOf(entry("a", { publisher: "Acme Tools *" })), { name: "Acme Tools", verified: false });
});

test("the mock turns user entries off and back on, never all-users ones", () => {
  resetStartupMock();
  const list = startupHandlers.startup_list({}) as StartupEntry[];
  const user = list.find((e) => e.scope === "user" && e.enabled)!;
  const off = startupHandlers.startup_set({ id: user.id, enabled: false }) as StartupEntry;
  assert.equal(off.enabled, false);
  assert.ok(off.disabledAt);
  const on = startupHandlers.startup_set({ id: user.id, enabled: true }) as StartupEntry;
  assert.equal(on.enabled, true);
  assert.equal(on.disabledAt, null);
  const machine = list.find((e) => e.scope === "machine")!;
  assert.throws(() => startupHandlers.startup_set({ id: machine.id, enabled: false }), (e: { code: string }) => e.code === "needs-admin");
  assert.throws(() => startupHandlers.startup_set({ id: "nope", enabled: false }), (e: { code: string }) => e.code === "not-found");
});

test("the Recycle Bin mock empties", async () => {
  const before = recycleBinHandlers.recyclebin_status({}) as { items: number; drives: unknown[] };
  assert.ok(before.items > 0 && before.drives.length > 0);
  const r = (await recycleBinHandlers.recyclebin_empty({})) as { items: number; failed: string[] };
  assert.equal(r.items, before.items);
  assert.deepEqual(r.failed, []);
  const after = recycleBinHandlers.recyclebin_status({}) as { items: number; drives: unknown[] };
  assert.equal(after.items, 0);
  assert.equal(after.drives.length, 0);
});

test("every dynamic key exists in both languages", () => {
  const dynamic = [
    ...["all", "enabled", "disabled", "user", "machine"].map((f) => `su-filter-${f}`),
    ...["name", "status"].map((s) => `su-sort-${s}`),
    ...["user-run", "machine-run", "machine-run32", "user-folder", "common-folder"].map((o) => `su-origin-${o}`),
    ...["windows", "security"].flatMap((c) => [`su-caution-${c}`, `su-caution-${c}-hint`, `su-confirm-${c}`]),
    "su-enabled",
    "su-disabled",
    "su-undo-enabled",
    "su-undo-disabled",
    "activity-kind-recycle-bin",
  ];
  for (const lang of ["en", "vi"]) {
    const have = keys(lang);
    for (const k of dynamic) assert.ok(have.has(k), `${lang}: ${k}`);
  }
});
