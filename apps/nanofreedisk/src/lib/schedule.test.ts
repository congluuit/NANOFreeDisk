// Automatic cleaning (docs/SCHEDULE.md): the settings helpers, the browser mock
// and a message for every key the section builds at run time.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { DEFAULT_CONFIG, NOTE_KEYS, categoryLabel, freeUpView, noteTexts, sanitize, setCategory } from "./schedule/model.ts";
import { resetScheduleMock, scheduleHandlers, setScheduleRecorder } from "./schedule/mock.ts";
import type { SchedulePreview, ScheduleState } from "./schedule/types.ts";

function keys(lang: string): Set<string> {
  const text = readFileSync(new URL(`../../../../locales/${lang}/schedule.ftl`, import.meta.url), "utf8");
  return new Set([...text.matchAll(/^([a-z][\w-]*)\s*=/gm)].map((m) => m[1]));
}

test("everything starts off", () => {
  assert.equal(DEFAULT_CONFIG.frequency, "off");
  assert.equal(DEFAULT_CONFIG.lowSpace, false);
  assert.deepEqual(DEFAULT_CONFIG.offCategories, []);
});

test("free up space opens Quick clean, else Clean, else Home", () => {
  assert.equal(freeUpView(true, true), "quick");
  assert.equal(freeUpView(false, true), "clean");
  assert.equal(freeUpView(false, false), "home");
});

test("categories are only ever left out", () => {
  assert.deepEqual(setCategory([], "system", false), ["system"]);
  assert.deepEqual(setCategory(["system", "gpu"], "system", true), ["gpu"]);
  // ticking twice does not duplicate
  assert.deepEqual(setCategory(["gpu"], "gpu", false), ["gpu"]);
  const t = (id: string) => (id === "schedule-cat-gpu" ? "Graphics shader caches" : id);
  assert.equal(categoryLabel("gpu", t), "Graphics shader caches");
  assert.equal(categoryLabel("new-kind", t), "new-kind");
});

test("numbers are pulled into the backend's ranges", () => {
  const c = sanitize({ ...DEFAULT_CONFIG, lowPercent: 900, lowGb: Number.NaN, offCategories: ["", "gpu"] });
  assert.equal(c.lowPercent, 50);
  assert.equal(c.lowGb, 10);
  assert.deepEqual(c.offCategories, ["gpu"]);
  assert.equal(sanitize({ ...DEFAULT_CONFIG, lowPercent: 0 }).lowPercent, 1);
});

test("notification texts keep the backend's placeholders", () => {
  const t = (id: string, args?: Record<string, string | number>) => `${id}:${JSON.stringify(args ?? {})}`;
  const texts = noteTexts(t);
  assert.deepEqual(Object.keys(texts).sort(), ["auto-body", "auto-title", "low-body", "low-title", "tray-free-up"]);
  assert.ok(texts["auto-body"].includes('"size":"{size}"'));
  assert.ok(texts["low-body"].includes('"free":"{free}"'));
});

test("every key built at run time exists in EN and VI", () => {
  const needed = [
    ...["off", "weekly", "monthly"].map((f) => `schedule-freq-${f}`),
    ...Object.values(NOTE_KEYS).map(([id]) => id),
    "activity-kind-clean-scheduled",
    ...["ai-agent", "ai-app", "ai-ide", "browser", "editor", "game-engine", "gpu", "ide", "llm-runtime", "ml-cache", "package-manager", "sdk", "system", "tool", "toolchain"].map(
      (c) => `schedule-cat-${c}`,
    ),
  ];
  for (const lang of ["en", "vi"]) {
    const have = keys(lang);
    for (const k of needed) assert.ok(have.has(k), `${lang}: ${k}`);
  }
});

test("the mock previews, then cleans exactly that into the history", () => {
  resetScheduleMock();
  const recorded: { kind: string; count: number; mode: string }[] = [];
  setScheduleRecorder((kind, items, mode) => recorded.push({ kind, count: items.length, mode }));
  const h = scheduleHandlers;
  let s = h.schedule_get({}) as ScheduleState;
  assert.equal(s.config.frequency, "off");
  assert.equal(s.next, null);
  s = h.schedule_set({ config: { ...s.config, frequency: "weekly", offCategories: ["gpu"] } }) as ScheduleState;
  assert.ok(s.next && s.next > Date.now() / 1000);
  // "Run now" without a preview is refused
  assert.throws(() => h.schedule_run_now({}), (e: { code: string }) => e.code === "no-preview");
  const p = h.schedule_preview({}) as SchedulePreview;
  assert.ok(p.rules.every((r) => r.category !== "gpu"));
  assert.equal(p.bytes, p.rules.reduce((sum, r) => sum + r.bytes, 0));
  const r = h.schedule_run_now({}) as { freed: number };
  assert.equal(r.freed, p.bytes);
  assert.deepEqual(recorded, [{ kind: "clean-scheduled", count: p.rules.length, mode: "quarantine" }]);
  s = h.schedule_get({}) as ScheduleState;
  assert.equal(s.last?.freed, p.bytes);
  const a = h.autostart_set({ enabled: true }) as { on: boolean };
  assert.equal(a.on, true);
});
