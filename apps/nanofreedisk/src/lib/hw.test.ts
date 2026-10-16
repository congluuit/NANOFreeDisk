import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { asText, sections, tempLevel } from "./hw/overview.ts";
import { hwHandlers } from "./hw/mock.ts";
import type { HwInventory } from "./hw/types.ts";

/** Message ids (and their variables) of locales/<lang>/hwmon.ftl. */
function messages(lang: string): Map<string, Set<string>> {
  const text = readFileSync(new URL(`../../../../locales/${lang}/hwmon.ftl`, import.meta.url), "utf8");
  const out = new Map<string, Set<string>>();
  for (const line of text.split(/\r?\n/)) {
    const m = /^([a-z][\w-]*)\s*=(.*)$/.exec(line);
    if (m) out.set(m[1], new Set([...m[2].matchAll(/\{\s*\$([\w-]+)/g)].map((v) => v[1])));
  }
  return out;
}

const inv = hwHandlers.hw_inventory({}) as HwInventory;

for (const lang of ["en", "vi"]) {
  test(`overview only uses keys defined in ${lang}/hwmon.ftl, with their variables`, () => {
    const msgs = messages(lang);
    const t = (id: string, args?: Record<string, string | number>) => {
      assert.ok(msgs.has(id), `missing ${id}`);
      for (const v of msgs.get(id)!) assert.ok(args && v in args, `${id}: variable ${v} not passed`);
      return id;
    };
    const list = sections(inv, t, { size: String, number: String });
    assert.ok(list.length >= 8);
  });
}

test("overview sections and copy-as-text agree", () => {
  const t = (id: string, args?: Record<string, string | number>) => (args ? `${id}(${Object.values(args).join(",")})` : id);
  const list = sections(inv, t, { size: (b) => `${Math.round(b / 2 ** 30)}G`, number: String });
  const cpu = list.find((s) => s.id === "cpu")!;
  assert.equal(cpu.items[0].heading, "13th Gen Intel(R) Core(TM) i9-13900HX");
  assert.ok(cpu.items[0].rows.some((r) => r.value === "hw-cores-hybrid(24,8,16)"));
  const mem = list.find((s) => s.id === "memory")!;
  assert.equal(mem.items.length, 3, "summary + two sticks");
  const disks = list.find((s) => s.id === "storage")!;
  assert.ok(disks.items[0].rows.some((r) => r.temp?.kind === "disk"));
  const simple = asText(list, false, "HEAD");
  const full = asText(list, true, "HEAD");
  assert.ok(simple.startsWith("HEAD"));
  assert.ok(!simple.includes("hw-computer") && full.includes("hw-computer"), "expert rows only in expert text");
  assert.ok(full.includes("AVX2"));
});

test("temperature levels", () => {
  assert.equal(tempLevel(45, "disk"), "ok");
  assert.equal(tempLevel(65, "disk"), "warm");
  assert.equal(tempLevel(70, "disk"), "hot");
  assert.equal(tempLevel(94, "cpu"), "warm");
  assert.equal(tempLevel(100, "cpu"), "hot");
});
