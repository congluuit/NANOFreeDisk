// node --test --experimental-strip-types src/lib/*.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";

import { currentOs, hasFeature, modKey, viewAvailable } from "./platform.ts";

test("macOS hides the Windows-only views, Windows and the mock keep everything", () => {
  for (const view of ["clean", "health", "hardware", "monitor", "programs"] as const) {
    assert.equal(viewAvailable("macos", view), false, view);
    assert.equal(viewAvailable("windows", view), true, view);
    assert.equal(viewAvailable("browser", view), true, view);
  }
  for (const view of ["home", "analyze", "duplicates", "oldlarge", "tidy", "devcache", "activity", "settings"] as const) {
    assert.equal(viewAvailable("macos", view), true, view);
  }
  assert.equal(hasFeature("macos", "shell-menu"), false);
  assert.equal(hasFeature("windows", "sensors"), true);
});

test("the reported OS wins over the user agent", () => {
  assert.equal(currentOs("macos"), "macos");
  assert.equal(currentOs("windows"), "windows");
  // no answer yet: Node's user agent is no Mac
  assert.equal(currentOs(null), "windows");
  assert.equal(modKey("macos"), "⌘");
  assert.equal(modKey("windows"), "Ctrl");
});
