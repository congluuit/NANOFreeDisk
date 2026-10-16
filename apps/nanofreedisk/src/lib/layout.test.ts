// node --test --experimental-strip-types src/lib/*.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";

import { hitTest, layoutTreemap, minBytesFor } from "./treemap.ts";
import { hitArc, layoutSunburst } from "./sunburst.ts";
import { formatSize, parseSize } from "./format.ts";
import type { Slice } from "./types.ts";

function slice(): Slice {
  // 0 root(100) → 1 dir(60) → 3 a(40), 4 b(20) ; 2 file(40)
  return {
    ids: [10, 11, 12, 13, 14],
    parents: [-1, 0, 0, 1, 1],
    sizes: [100, 60, 40, 40, 20],
    kinds: [1, 1, 0, 0, 0],
    names: ["root", "dir", "f", "a", "b"],
    depths: [0, 1, 1, 2, 2],
    truncated: false,
  };
}

test("treemap tiles fill the bounds proportionally", () => {
  const s = slice();
  const l = layoutTreemap(s, { x: 0, y: 0, w: 100, h: 100 }, { pad: 0, header: 0 });
  assert.equal(l.count, 5);
  const area = (i: number) => l.w[i] * l.h[i];
  assert.ok(Math.abs(area(1) - 6000) < 1);
  assert.ok(Math.abs(area(2) - 4000) < 1);
  assert.ok(Math.abs(area(3) + area(4) - 6000) < 1);
  // children stay inside their parent
  for (const c of [3, 4]) {
    assert.ok(l.x[c] >= l.x[1] - 1e-3 && l.x[c] + l.w[c] <= l.x[1] + l.w[1] + 1e-3);
    assert.ok(l.y[c] >= l.y[1] - 1e-3 && l.y[c] + l.h[c] <= l.y[1] + l.h[1] + 1e-3);
  }
  // hit test returns the deepest tile
  const hit = hitTest(l, l.x[3] + 1, l.y[3] + 1);
  assert.equal(hit, 3);
  assert.equal(hitTest(l, -5, -5), -1);
});

test("treemap aspect ratios stay reasonable", () => {
  const n = 200;
  const s: Slice = { ids: [0], parents: [-1], sizes: [0], kinds: [1], names: ["r"], depths: [0], truncated: false };
  for (let i = 1; i <= n; i++) {
    s.ids.push(i);
    s.parents.push(0);
    s.sizes.push(1000 / i);
    s.kinds.push(0);
    s.names.push(`f${i}`);
    s.depths.push(1);
    s.sizes[0] += 1000 / i;
  }
  const l = layoutTreemap(s, { x: 0, y: 0, w: 800, h: 600 });
  let worst = 0;
  for (let i = 1; i <= 50; i++) worst = Math.max(worst, Math.max(l.w[i] / l.h[i], l.h[i] / l.w[i]));
  assert.ok(worst < 4, `worst aspect ratio ${worst}`);
});

test("sunburst angles are proportional and hit-testable", () => {
  const a = layoutSunburst(slice(), 4);
  const span = (i: number) => a.end[i] - a.start[i];
  assert.ok(Math.abs(span(1) - Math.PI * 2 * 0.6) < 1e-9);
  assert.ok(Math.abs(span(3) - Math.PI * 2 * 0.4) < 1e-9);
  assert.equal(hitArc(a, a.start[2] + 0.01, 1), 2);
  assert.equal(hitArc(a, 0.01, 2), 3);
  assert.equal(hitArc(a, 0.1, 0), 0);
});

test("size helpers", () => {
  assert.equal(formatSize(512), "512 B");
  assert.equal(formatSize(1536), "1.5 KB");
  assert.equal(formatSize(10 * 2 ** 30), "10.0 GB");
  assert.equal(parseSize("10MB"), 10 * 2 ** 20);
  assert.equal(parseSize("1.5 g"), 1.5 * 2 ** 30);
  assert.equal(parseSize("abc"), null);
  assert.equal(minBytesFor(1000, 10, 10, 1), 10);
});
