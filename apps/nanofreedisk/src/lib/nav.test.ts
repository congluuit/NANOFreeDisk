// The sidebar in Simple and Expert mode, the Tools page and the views a platform
// hides: every view stays reachable in both modes.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { EXPERT_NAV, SIMPLE_NAV, TOOLS, activeEntry, isGroup, sidebar, toolCards, type NavEntry } from "./nav.ts";
import { viewAvailable } from "./platform.ts";
import type { View } from "./state.svelte.ts";

const flat = (entries: NavEntry[]): View[] => entries.flatMap((e) => (isGroup(e) ? e.items.map((i) => i.id) : [e.id]));
const ALL: View[] = ["home", "analyze", "oldlarge", "tidy", "clean", "activity", "hardware", "devcache", "duplicates", "monitor", "health", "programs", "startup", "settings", "quick", "tools"];

test("Simple mode shows the short sidebar", () => {
  assert.deepEqual(flat(sidebar("simple", () => true)), ["home", "quick", "analyze", "tools"]);
});

test("Expert mode keeps every tool, grouped, plus Quick clean", () => {
  const views = flat(sidebar("expert", () => true));
  assert.ok(views.includes("quick"));
  for (const c of TOOLS) assert.ok(views.includes(c.id), c.id);
  assert.equal(sidebar("expert", () => true).filter(isGroup).length, 2);
});

test("every view is reachable in both modes (sidebar, Tools, fixed entries)", () => {
  // the cleanup history and Settings are fixed entries of the sidebar
  const fixed: View[] = ["activity", "settings"];
  for (const mode of ["simple", "expert"] as const) {
    const reach = new Set<View>([...flat(sidebar(mode, () => true)), ...fixed, ...(mode === "simple" ? TOOLS.map((c) => c.id) : [])]);
    for (const v of ALL.filter((x) => x !== "tools" || mode === "simple")) assert.ok(reach.has(v), `${mode}: ${v}`);
  }
});

test("the Tools page lists what this platform has", () => {
  const mac = toolCards((v) => viewAvailable("macos", v)).map((c) => c.id);
  assert.ok(!mac.includes("clean") && !mac.includes("health") && !mac.includes("hardware") && !mac.includes("monitor") && !mac.includes("programs"));
  assert.ok(mac.includes("duplicates") && mac.includes("devcache"));
  assert.equal(toolCards((v) => viewAvailable("windows", v)).length, TOOLS.length);
  // a group whose tools are all missing is left out
  assert.ok(sidebar("expert", (v) => viewAvailable("macos", v)).every((e) => !isGroup(e) || e.items.length > 0));
});

test("in Simple mode a tool lights up Tools", () => {
  assert.equal(activeEntry("simple", "clean"), "tools");
  assert.equal(activeEntry("simple", "quick"), "quick");
  assert.equal(activeEntry("expert", "clean"), "clean");
});

test("nav labels and tool sentences exist in both languages", () => {
  const keys = (lang: string) => {
    const out = new Set<string>();
    for (const f of ["app.ftl", "quick.ftl", "clean.ftl", "programs.ftl", "startup.ftl", "hwmon.ftl"]) {
      for (const line of readFileSync(new URL(`../../../../locales/${lang}/${f}`, import.meta.url), "utf8").split(/\r?\n/)) {
        const m = /^([a-z][\w-]*)\s*=/.exec(line);
        if (m) out.add(m[1]);
      }
    }
    return out;
  };
  const needed = [
    ...[...EXPERT_NAV, ...SIMPLE_NAV].flatMap((e) => (isGroup(e) ? [e.label, ...e.items.map((i) => i.label)] : [e.label])),
    ...TOOLS.flatMap((c) => [c.label, c.when]),
  ];
  for (const lang of ["en", "vi"]) {
    const have = keys(lang);
    const missing = needed.filter((k) => !have.has(k));
    assert.deepEqual(missing, [], `${lang}: ${missing.join(", ")}`);
  }
});
