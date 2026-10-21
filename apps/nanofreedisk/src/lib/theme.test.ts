import { test } from "node:test";
import assert from "node:assert/strict";

import {
  PRESETS,
  contrast,
  contrastIssues,
  exportToml,
  importToml,
  paletteVars,
  resolvePalette,
  type PresetId,
} from "./theme.ts";
import { hex6, tileStyle } from "./mapstyle.ts";

test("built-in presets meet WCAG AA for text", () => {
  for (const [id, p] of Object.entries(PRESETS)) {
    assert.deepEqual(contrastIssues(p.colors), [], `${id} has low-contrast pairs`);
  }
});

test("theme.toml round-trips", () => {
  const colors = { ...PRESETS.nord.colors, accent: "#123456" };
  const text = exportToml("Mine", "nord", colors);
  const back = importToml(text);
  assert.ok(back);
  assert.equal(back.name, "Mine");
  assert.equal(back.preset, "nord" as PresetId);
  assert.equal(back.overrides.accent, "#123456");
  assert.equal(back.overrides["tm-dir"], colors["tm-dir"]);
  assert.equal(importToml("garbage = 1"), null);
});

test("system mode follows the Windows accent but keeps it readable", () => {
  const dark = resolvePalette({ mode: "system", preset: "nano-dark", overrides: {} }, true, "#0078d4");
  assert.equal(dark.dark, true);
  assert.ok(contrast(dark.colors.accent, dark.colors.surface) >= 3);
  const light = resolvePalette({ mode: "system", preset: "nano-dark", overrides: {} }, false, null);
  assert.equal(light.colors.bg, PRESETS["nano-light"].colors.bg);
});

test("custom mode ignores invalid overrides", () => {
  const r = resolvePalette(
    { mode: "custom", preset: "solarized", overrides: { bg: "red", text: "#111111" } },
    true,
    null,
  );
  assert.equal(r.colors.bg, PRESETS.solarized.colors.bg);
  assert.equal(r.colors.text, "#111111");
  assert.equal(r.dark, false);
});

test("treemap tiles and frames keep their labels readable in every preset", () => {
  const tokens = ["tm-1", "tm-2", "tm-3", "tm-4", "tm-5", "tm-6", "tm-7", "tm-8", "tm-other", "risk-safe", "risk-caution", "risk-advanced"] as const;
  for (const [id, p] of Object.entries(PRESETS)) {
    for (const k of tokens) {
      const st = tileStyle(p.colors[k], p.colors.surface, p.dark);
      assert.ok(contrast(st.ink, st.fill) >= 4.5, `${id} ${k} tile label ${contrast(st.ink, st.fill).toFixed(2)}`);
      assert.ok(contrast(st.frameInk, st.frame) >= 4.5, `${id} ${k} frame label ${contrast(st.frameInk, st.frame).toFixed(2)}`);
    }
  }
});

test("pills keep their text readable in every preset", () => {
  for (const [id, p] of Object.entries(PRESETS)) {
    const v = paletteVars(p.colors, p.dark);
    for (const r of ["risk-safe", "risk-caution", "risk-advanced", "risk-protected"]) {
      assert.ok(contrast(v[`${r}-ink`], v[`${r}-soft`]) >= 4.5, `${id} ${r}`);
    }
    assert.ok(contrast(v["accent-ink"], v["accent-soft"]) >= 4.5, `${id} accent`);
  }
});

test("hex6 normalizes CSS colors", () => {
  assert.equal(hex6("#ABC"), "#aabbcc");
  assert.equal(hex6(" #12abEF "), "#12abef");
  assert.equal(hex6("rgb(1, 2, 3)"), "#888888");
});
