import { test } from "node:test";
import assert from "node:assert/strict";

import {
  PRESETS,
  contrast,
  contrastIssues,
  exportToml,
  importToml,
  resolvePalette,
  type PresetId,
} from "./theme.ts";

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
