// AI leftovers tab (docs/AI_LEFTOVERS.md): selection helpers, the browser mock and
// the messages every code the backend can send needs.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { AI_BLOCKED, AI_KEEP, aiBytes, aiPreselected } from "./devcache/ai.ts";
import { devHandlers } from "./devcache/mock.ts";
import type { AiPlan, AiReport } from "./devcache/types.ts";

function keys(lang: string): Set<string> {
  const out = new Set<string>();
  for (const file of ["ai-leftovers.ftl", "devcache.ftl", "clean.ftl", "app.ftl"]) {
    const text = readFileSync(new URL(`../../../../locales/${lang}/${file}`, import.meta.url), "utf8");
    for (const line of text.split(/\r?\n/)) {
      const m = /^([a-z][\w-]*)\s*=/.exec(line);
      if (m) out.add(m[1]);
    }
  }
  return out;
}

test("every blocked / keep code and item kind has a message in both languages", () => {
  for (const lang of ["en", "vi"]) {
    const k = keys(lang);
    for (const code of AI_BLOCKED) assert.ok(k.has(`reason-${code}`), `${lang}: reason-${code}`);
    for (const code of AI_KEEP) assert.ok(k.has(`ail-keep-${code}`), `${lang}: ail-keep-${code}`);
    for (const id of ["dev-tab-ai", "activity-kind-ai-leftovers", "activity-kind-worktree", "health-ai-leftovers", "health-fix-devcache"]) {
      assert.ok(k.has(id), `${lang}: ${id}`);
    }
  }
});

test("finished sessions: only pre-selected items and build output, never kept ones", () => {
  const r = devHandlers.ai_leftovers_scan({}) as AiReport;
  const picked = aiPreselected(r);
  const byPath = new Map(r.items.map((i) => [i.path, i]));
  for (const p of picked) {
    const i = byPath.get(p);
    if (i) {
      assert.ok(i.selected && !i.blocked && !i.keep, p);
    } else {
      // build output of a worktree that cannot be removed
      assert.ok(r.items.some((x) => x.worktree?.artifacts.some((a) => a.path === p && a.selected)), p);
    }
  }
  const open = r.items.find((i) => i.keep === "session-open");
  assert.ok(open && !picked.includes(open.path), "an open session's worktree is never pre-selected");
  const dirty = r.items.find((i) => i.blocked === "dirty");
  assert.ok(dirty && !picked.includes(dirty.path), "a dirty worktree is never picked");
  assert.ok(aiBytes(r, picked) > 0);
  assert.equal(aiBytes(r, []), 0);
  const plan = devHandlers.ai_leftovers_plan({ paths: picked }) as AiPlan;
  assert.ok(plan.commands.every((c) => c.includes("worktree remove")));
});
