// Programs view (docs/UNINSTALLER.md): list filtering and sorting, the selection
// items of leftovers and software junk, the browser mock, and a message for every
// code the backend can send.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { LEFTOVER_GROUPS, groupOf, initial, junkItems, leftoverItems, shortHive, uninstallCommand, visiblePrograms } from "./programs/model.ts";
import { programsHandlers, resetProgramsMock } from "./programs/mock.ts";
import type { JunkReport, LeftoverReport, Program } from "./programs/types.ts";

function keys(lang: string): Set<string> {
  const out = new Set<string>();
  for (const file of ["programs.ftl", "clean.ftl", "app.ftl", "ai-leftovers.ftl"]) {
    const text = readFileSync(new URL(`../../../../locales/${lang}/${file}`, import.meta.url), "utf8");
    for (const line of text.split(/\r?\n/)) {
      const m = /^([a-z][\w-]*)\s*=/.exec(line);
      if (m) out.add(m[1]);
    }
  }
  return out;
}

const list = () => programsHandlers.programs_list({ store: true }) as Program[];

test("search, filters and sorting", () => {
  resetProgramsMock();
  const all = list();
  assert.ok(all.length >= 8);
  assert.deepEqual(visiblePrograms(all, "valve", "all", "name").map((p) => p.name), ["Steam"]);
  assert.deepEqual(visiblePrograms(all, "visual redist", "all", "name").length, 1);
  assert.ok(visiblePrograms(all, "", "store", "name").every((p) => p.source === "store"));
  assert.ok(visiblePrograms(all, "", "win32", "name").every((p) => p.source !== "store"));
  assert.ok(visiblePrograms(all, "", "user", "name").every((p) => p.scope === "user"));
  assert.deepEqual(visiblePrograms(all, "", "broken", "name").map((p) => p.name), ["Old Game Launcher"]);
  const bySize = visiblePrograms(all, "", "all", "size");
  assert.equal(bySize[0].name, "Steam");
  assert.ok(bySize.every((p, i) => i === 0 || bySize[i - 1].size >= p.size));
  const byDate = visiblePrograms(all, "", "large", "date");
  assert.ok(byDate.every((p) => p.size >= 1024 ** 3));
  assert.ok(byDate.every((p, i) => i === 0 || (byDate[i - 1].installedAt ?? 0) >= (p.installedAt ?? 0)));
});

test("small helpers", () => {
  assert.equal(initial("7-Zip"), "7");
  assert.equal(initial("  ứng dụng"), "Ứ");
  assert.equal(initial("—"), "?");
  assert.equal(shortHive("HKEY_LOCAL_MACHINE\\SOFTWARE\\Acme"), "HKLM\\SOFTWARE\\Acme");
  assert.equal(shortHive("HKEY_CURRENT_USER\\Software\\Acme"), "HKCU\\Software\\Acme");
  const p = list()[0];
  assert.match(uninstallCommand(p), /^nfd programs uninstall "/);
  assert.match(uninstallCommand({ ...p, broken: true }), /^nfd programs leftovers ".*" --apply$/);
});

test("uninstall, leftovers and their selection", async () => {
  resetProgramsMock();
  const discord = list().find((p) => p.name === "Discord")!;
  const out = (await programsHandlers.programs_uninstall({ id: discord.id, quiet: false })) as { status: string };
  assert.equal(out.status, "removed");
  assert.ok(!list().some((p) => p.id === discord.id), "gone from the list");
  const r = programsHandlers.programs_leftovers({ id: discord.id, forced: false }) as LeftoverReport;
  const items = leftoverItems(r);
  assert.equal(items.length, r.items.length);
  const svc = r.items.find((i) => i.kind === "service")!;
  assert.equal(items.find((i) => i.id === svc.id)?.locked, true, "services are information only");
  assert.equal(groupOf(svc), "services");
  assert.ok(r.items.every((i) => LEFTOVER_GROUPS.some((g) => g.id === groupOf(i))));
  const picked = r.items.filter((i) => i.selected).map((i) => i.id);
  assert.ok(picked.length > 0);
  assert.ok(r.items.filter((i) => i.level === "caution").every((i) => !i.selected), "caution never pre-selected");
  const done = programsHandlers.programs_leftovers_remove({ id: discord.id, picked }) as { done: number };
  assert.equal(done.done, picked.length);
  // a broken entry goes straight to its leftovers
  const broken = list().find((p) => p.broken)!;
  const b = (await programsHandlers.programs_uninstall({ id: broken.id, quiet: false })) as { status: string };
  assert.equal(b.status, "broken");
  assert.equal((programsHandlers.programs_leftovers({ id: broken.id, forced: false }) as LeftoverReport).mode, "broken");
});

test("software junk selection and clean", () => {
  resetProgramsMock();
  const r = programsHandlers.programs_junk_scan({}) as JunkReport;
  const items = junkItems(r);
  assert.equal(items.length, r.groups.reduce((s, g) => s + g.items.length, 0));
  const discord = r.groups.find((g) => g.name === "Discord")!;
  assert.ok(discord.running);
  assert.ok(items.filter((i) => i.group === discord.key).every((i) => i.skip === "discord.exe"), "running app: likely skipped");
  assert.equal(junkItems(null).length, 0);
  const safe = r.groups.flatMap((g) => g.items).filter((i) => i.selected).map((i) => i.path);
  const out = programsHandlers.programs_junk_clean({ paths: safe }) as { done: number; freed: number };
  assert.equal(out.done, safe.length);
  const after = programsHandlers.programs_junk_scan({}) as JunkReport;
  assert.ok(after.groups.flatMap((g) => g.items).every((i) => !safe.includes(i.path)));
});

test("every code the backend sends has a message in both languages", () => {
  const needed = [
    "nav-programs",
    ...["installed", "junk"].map((x) => `pg-tab-${x}`),
    ...["all", "win32", "store", "user", "large", "broken"].map((x) => `pg-filter-${x}`),
    ...["name", "size", "date"].map((x) => `pg-sort-${x}`),
    ...["runtime", "driver", "windows"].flatMap((x) => [`pg-caution-${x}`, `pg-caution-${x}-hint`]),
    ...["self", "webview2", "no-remove", "no-uninstaller", "non-removable"].map((x) => `pg-blocked-${x}`),
    ...["after-uninstall", "broken", "forced"].map((x) => `pg-left-title-${x}`),
    ...LEFTOVER_GROUPS.map((g) => `pg-left-group-${g.id}`),
    ...[
      "install-dir", "same-name", "vendor-folder", "weak-name", "shortcut", "start-menu-folder", "arp-entry",
      "software-key", "vendor-key", "app-path", "run-entry", "service", "store-data",
    ].map((x) => `pg-reason-${x}`),
    ...[
      "chromium-cache", "service-worker", "electron-logs", "crashpad", "crash-dumps", "crashes", "dump-file",
      "logs", "temp", "old-version", "update-package", "update-temp", "pending-update",
    ].map((x) => `pg-junk-what-${x}`),
    ...["cache", "crash", "log", "temp", "update"].map((x) => `pg-junk-impact-${x}`),
    // blocked codes and skip reasons of leftovers, junk and the elevated step
    ...[
      "other-program", "in-use", "contains-history", "too-large", "info", "registry-protected", "not-all-users",
      "uac-declined", "elevation-failed", "backup", "delete", "access-denied", "missing", "registry",
      "unexpected-key", "unexpected-value", "protected", "system-dir", "volume-root", "contains-protected",
    ].map((x) => `reason-${x}`),
    "pg-left-large",
    "activity-kind-uninstall",
    "activity-kind-software-junk",
    "activity-mode-registry",
    "activity-restore-needs-admin",
    "activity-restore-admin-confirm",
  ];
  for (const lang of ["en", "vi"]) {
    const have = keys(lang);
    const missing = needed.filter((k) => !have.has(k));
    assert.deepEqual(missing, [], `${lang}: ${missing.join(", ")}`);
  }
});
