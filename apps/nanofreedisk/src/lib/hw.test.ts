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

import { readdirSync } from "node:fs";
import { formatValue, homeTemps, sensorLabel, tree } from "./hw/sensors.ts";
import type { SensorSnapshot } from "./hw/types.ts";

/** Sensor labels and note reasons the Rust providers emit. */
function rustLabels(): { labels: Set<string>; reasons: Set<string> } {
  const labels = new Set<string>();
  const reasons = new Set<string>();
  const dirs = ["../../../../crates/nfd-sensors/src/", "../../../../crates/nfd-sensors/src/win/"];
  for (const d of dirs) {
    const dir = new URL(d, import.meta.url);
    for (const f of readdirSync(dir).filter((x) => x.endsWith(".rs"))) {
      const text = readFileSync(new URL(f, dir), "utf8");
      for (const m of text.matchAll(/Kind::\w+,\s*"([a-z0-9-]+)"/g)) labels.add(m[1]);
      for (const m of text.matchAll(/^const GROUPS[^\n]*/gm)) for (const g of m[0].matchAll(/"([a-z0-9-]+)"/g)) labels.add(g[1]);
      for (const m of text.matchAll(/reason:\s*"([a-z-]+)"/g)) reasons.add(m[1]);
    }
  }
  return { labels, reasons };
}

for (const lang of ["en", "vi"]) {
  test(`every Rust sensor label and note has a ${lang} message`, () => {
    const msgs = messages(lang);
    const { labels, reasons } = rustLabels();
    assert.ok(labels.size > 30, `${labels.size} labels found`);
    for (const l of labels) assert.ok(msgs.has(`hw-s-${l}`), `hw-s-${l}`);
    for (const r of reasons) {
      assert.ok(msgs.has(`hw-note-${r}`), `hw-note-${r}`);
      assert.ok(msgs.has(`home-temp-why-${r}`), `home-temp-why-${r}`);
    }
  });
}

const snap = hwHandlers.sensors_snapshot({}) as SensorSnapshot;

test("sensor tree groups by device and kind; Simple keeps primary sensors", () => {
  const full = tree(snap, false);
  const simple = tree(snap, true);
  const cpu = full.find((d) => d.device.id === "cpu")!;
  assert.deepEqual(
    cpu.groups.map((g) => g.kind),
    ["voltage", "power", "clock", "temperature", "load"],
    "PawnIO notes keep their groups visible",
  );
  const cpuSimple = simple.find((d) => d.device.id === "cpu")!;
  const load = cpuSimple.groups.find((g) => g.kind === "load")!;
  assert.equal(load.sensors.length, 1, "threads hidden in Simple");
  const t = (id: string, a?: Record<string, string | number>) => `${id}${a?.n ? `#${a.n}` : ""}`;
  const thread = snap.sensors.find((s) => s.label === "cpu-thread")!;
  assert.equal(sensorLabel(thread, t), "hw-s-cpu-thread#1 · hw-core-p");
});

test("sensor values format per kind", () => {
  assert.equal(formatValue("temperature", 52.04), "52.0 °C");
  assert.equal(formatValue("voltage", 1.2), "1.200 V");
  assert.equal(formatValue("throughput", 1536), "1.5 KB/s");
  assert.equal(formatValue("energy", 59814), "59.8 Wh");
  assert.equal(formatValue("load", null), "—");
});

test("home card picks GPU / hottest disk and explains a missing CPU temperature", () => {
  const [cpu, gpu, disk] = homeTemps(snap);
  assert.equal(cpu.value, null);
  assert.equal(cpu.reason, "needs-pawnio");
  assert.ok(cpu.load != null);
  assert.equal(gpu.name, "NVIDIA GeForce RTX 4050 Laptop GPU");
  assert.equal(disk.name, "SAMSUNG MZVLB512HAJQ-000L7", "the hotter of the two disks");
  const asleep: SensorSnapshot = {
    ...snap,
    sensors: snap.sensors.filter((s) => s.device !== "gpu1"),
    notes: [...snap.notes, { device: "gpu1", kind: "temperature", reason: "gpu-asleep" }],
  };
  assert.equal(homeTemps(asleep)[1].reason, "gpu-asleep");
});
