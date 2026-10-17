// Analyze → Suggestions helpers (advice.ts): sections, selection items, titles,
// badges. Run with `pnpm test`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { badgeOf, blockedKey, defaultPicks, KNOWN_BLOCKED, sections, segments, selItems, titleOf, verdictOfCode, VERDICT_TOKEN, VERDICTS } from "./advice.ts";
import { needsAck, selectAll, summarize } from "./selection.ts";
import type { AdviceGroup, ByVerdict } from "./types";

const none: ByVerdict = { safe: 0, caution: 0, guide: 0, keep: 0, unknown: 0 };

function group(over: Partial<AdviceGroup>): AdviceGroup {
  return {
    key: "k",
    verdict: "safe",
    reason: "rule",
    level: "safe",
    source: "windows.temp.user",
    app: "Windows",
    title: null,
    impact: { en: "Temporary files.", vi: "Tệp tạm." },
    untouched: null,
    action: "clean",
    blocked: null,
    store: null,
    requiresClosed: [],
    bytes: 100,
    recent: 0,
    files: 3,
    count: 1,
    selected: true,
    actionable: true,
    location: null,
    elsewhere: false,
    items: [],
    ...over,
  };
}

test("sections split by verdict, personal media is 'big but keep'", () => {
  const gs = [
    group({ key: "a", verdict: "safe" }),
    group({ key: "b", verdict: "caution", level: "caution", selected: false }),
    group({ key: "c", verdict: "guide", action: "guide", actionable: false, selected: false, reason: "explained" }),
    group({ key: "d", verdict: "keep", action: "none", actionable: false, selected: false, reason: "system", bytes: 10 }),
    group({ key: "e", verdict: "unknown", action: "none", actionable: false, selected: false, reason: "personal", bytes: 50 }),
    group({ key: "f", verdict: "unknown", action: "none", actionable: false, selected: false, reason: "old-large" }),
  ];
  const s = sections(gs, [group({ key: "x", elsewhere: true }), group({ key: "y", elsewhere: true, verdict: "keep" })]);
  assert.deepEqual(s.safe.map((g) => g.key), ["a"]);
  assert.deepEqual(s.caution.map((g) => g.key), ["b"]);
  assert.deepEqual(s.guide.map((g) => g.key), ["c"]);
  // biggest first; only personal unknowns are listed
  assert.deepEqual(s.keep.map((g) => g.key), ["e", "d"]);
  assert.deepEqual(s.elsewhere.map((g) => g.key), ["x"]);
});

test("only runnable groups can be ticked; blocked ones are locked; caution needs the acknowledgement", () => {
  const gs = [
    group({ key: "safe" }),
    group({ key: "care", verdict: "caution", level: "caution", selected: false }),
    group({ key: "adv", verdict: "caution", level: "advanced", selected: false }),
    group({ key: "live", verdict: "safe", reason: "agent-worktree", action: "ai-leftovers", blocked: "git-check", actionable: false }),
    group({ key: "store", verdict: "caution", action: "store", actionable: false }),
    group({ key: "kept", verdict: "keep", action: "none", actionable: false }),
  ];
  const items = selItems(gs);
  assert.deepEqual(items.map((i) => i.id), ["safe", "care", "adv", "live"]);
  assert.equal(items.find((i) => i.id === "live")?.locked, true);
  assert.equal(items.find((i) => i.id === "adv")?.level, "advanced");
  // Simple mode: no Advanced, never the locked one
  assert.deepEqual(selectAll(items, false), ["safe", "care"]);
  assert.deepEqual(selectAll(items, true), ["safe", "care", "adv"]);
  assert.equal(needsAck(summarize(items, ["safe"], false)), false);
  assert.equal(needsAck(summarize(items, ["safe", "care"], false)), true);
  // pre-selected: 🟢 and actionable only
  assert.deepEqual(defaultPicks(gs), ["safe"]);
});

test("titles, blocked messages, badges and segments", () => {
  assert.deepEqual(titleOf(group({ reason: "build-output", source: "rust", app: null }), "en"), {
    key: "advice-reason-build-output",
    args: { app: "rust", kind: "Rust", title: "rust" },
  });
  const ex = titleOf(group({ reason: "explained", source: "winsxs", title: { en: "Component store", vi: "Kho thành phần" } }), "vi");
  assert.equal(ex.args.title, "Kho thành phần");
  assert.equal(blockedKey("session-live"), "advice-blocked-session-live");
  assert.equal(blockedKey("system-dir"), "advice-blocked-other");
  assert.deepEqual(badgeOf(undefined), { verdict: null, partly: false });
  assert.deepEqual(badgeOf({ verdict: "keep", reason: "system", root: true, inside: { ...none, keep: 5 }, reclaim: 0 }), { verdict: "keep", partly: false });
  assert.deepEqual(badgeOf({ verdict: null, reason: null, root: false, inside: { ...none, safe: 5, keep: 5 }, reclaim: 5 }), { verdict: null, partly: true });
  assert.deepEqual(badgeOf({ verdict: null, reason: null, root: false, inside: { ...none, unknown: 9 }, reclaim: 0 }), { verdict: "unknown", partly: false });
  const seg = segments({ ...none, safe: 1, keep: 3 });
  assert.deepEqual(seg.map((s) => [s.verdict, s.share]), [["safe", 0.25], ["keep", 0.75]]);
  assert.deepEqual(segments(none), []);
  assert.equal(verdictOfCode(3), "keep");
  assert.equal(verdictOfCode(255), null);
});

test("every verdict, reason, action and blocked code has its EN and VI text", () => {
  const en = readFileSync(new URL("../../../../locales/en/advice.ftl", import.meta.url), "utf8");
  const vi = readFileSync(new URL("../../../../locales/vi/advice.ftl", import.meta.url), "utf8");
  const keys = [
    ...VERDICTS.flatMap((v) => [`advice-verdict-${v}`, `advice-badge-${v}`, `advice-verdict-hint-${v}`]),
    ...["rule", "keep-rule", "build-output", "agent-worktree", "agent-leftover", "agent-history", "system", "programs", "protected", "personal", "explained", "old-large", "old-installer", "duplicate"].map((r) => `advice-reason-${r}`),
    ...["clean", "quarantine", "ai-leftovers", "recycle-bin", "store", "duplicates", "guide"].flatMap((a) => [`advice-action-${a}`, `advice-action-hint-${a}`]),
    ...KNOWN_BLOCKED.map((b) => `advice-blocked-${b}`),
    "advice-blocked-other",
    ...["safe", "caution", "guide", "keep", "elsewhere"].flatMap((s) => [`advice-section-${s}`, `advice-section-${s}-hint`]),
  ];
  for (const k of keys) {
    assert.match(en, new RegExp(`^${k} =`, "m"), `en ${k}`);
    assert.match(vi, new RegExp(`^${k} =`, "m"), `vi ${k}`);
  }
  // colors come from theme tokens only
  for (const v of VERDICTS) assert.match(VERDICT_TOKEN[v], /^--(risk|tm)-/);
});
