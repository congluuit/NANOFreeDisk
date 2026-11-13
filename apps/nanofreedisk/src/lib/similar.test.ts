// Similar photos: nothing is ever ticked by itself, "keep only the best" never
// ticks the best or a risky photo, a group can only lose every photo with its own
// "remove all" tick, the browser mock behaves like the backend, and every message
// the view asks for exists in both languages.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { allButBest, bestReason, prunePicks, resolution, selections, setGroupPicks, togglePick, totals, wouldEmpty, type Picks } from "./similar/logic.ts";
import { similarHandlers } from "./similar/mock.ts";
import type { Photo, SimilarGroup, SimilarReport } from "./similar/types.ts";

const PLAIN = { cloud: false, git: false, appDir: false, readonly: false };

function photo(path: string, width: number, height: number, size: number, extra: Partial<Photo> = {}): Photo {
  return { path, width, height, size, mtime: 0, volume: "0", fileId: null, flags: PLAIN, distance: 0, ...extra };
}

const group: SimilarGroup = {
  id: 1,
  extra: 0,
  photos: [
    photo("C:/Pictures/best.jpg", 4000, 3000, 4_000_000),
    photo("C:/Downloads/chat.jpg", 1280, 960, 200_000),
    photo("C:/OneDrive/edit.jpg", 4000, 3000, 2_000_000, { flags: { ...PLAIN, cloud: true } }),
  ],
};

test("keep only the best ticks every plain photo but the best", () => {
  assert.deepEqual(allButBest(group), ["C:/Downloads/chat.jpg"]);
  assert.equal(bestReason(group), "pixels");
  assert.equal(resolution(group.photos[0]), "4000 × 3000");
});

test("ticking and unticking, and the keep-none tick goes with an untick", () => {
  let picks: Picks = {};
  picks = togglePick(picks, 1, "C:/Downloads/chat.jpg");
  assert.deepEqual(picks[1], { remove: ["C:/Downloads/chat.jpg"], keepNone: false });
  picks = { 1: { remove: group.photos.map((p) => p.path), keepNone: true } };
  picks = togglePick(picks, 1, "C:/Pictures/best.jpg");
  assert.equal(picks[1].keepNone, false);
  assert.deepEqual(setGroupPicks(picks, 1, []), {});
});

test("a group never loses every photo without its own tick", () => {
  const all = group.photos.map((p) => p.path);
  assert.ok(wouldEmpty(group, all));
  const blocked = selections([group], { 1: { remove: all, keepNone: false } });
  assert.deepEqual(blocked, { ok: [], blocked: [1] });
  const confirmed = selections([group], { 1: { remove: all, keepNone: true } });
  assert.deepEqual(confirmed.ok, [{ group: 1, remove: all, keepNone: true }]);
  const t = totals([group], { 1: { remove: all, keepNone: true } });
  assert.equal(t.files, 3);
  assert.equal(t.bytes, 6_200_000);
  assert.equal(t.emptied, 1);
  assert.equal(t.risky.length, 1);
  assert.equal(totals([group], { 1: { remove: all, keepNone: false } }).blocked, 1);
});

test("picks of photos that left are dropped", () => {
  const smaller: SimilarGroup = { ...group, photos: group.photos.slice(0, 2) };
  const picks = prunePicks([smaller], { 1: { remove: ["C:/OneDrive/edit.jpg", "C:/Downloads/chat.jpg"], keepNone: false }, 7: { remove: ["x"], keepNone: false } });
  assert.deepEqual(picks, { 1: { remove: ["C:/Downloads/chat.jpg"], keepNone: false } });
});

test("the mock: strict finds less than loose, removal keeps a photo per group", () => {
  const strict = similarHandlers.similar_find({ paths: [], options: { personal: true, preset: "strict" } }) as SimilarReport;
  const loose = similarHandlers.similar_regroup({ preset: "loose" }) as SimilarReport;
  assert.ok(strict.groups.length > 0);
  assert.ok(loose.groups.length > strict.groups.length);
  for (const g of loose.groups) {
    assert.ok(g.photos.length > 1);
    assert.equal(g.photos[0].distance, 0);
  }
  const g = loose.groups[0];
  assert.throws(() => similarHandlers.similar_run({ selections: [{ group: g.id, remove: g.photos.map((p) => p.path) }], action: "recycle" }));
  const done = similarHandlers.similar_run({ selections: [{ group: g.id, remove: [g.photos[1].path] }], action: "recycle" }) as { done: number };
  assert.equal(done.done, 1);
});

function keys(lang: string): Set<string> {
  const text = readFileSync(new URL(`../../../../locales/${lang}/similar.ftl`, import.meta.url), "utf8");
  return new Set([...text.matchAll(/^([a-z][\w-]*) =/gm)].map((m) => m[1]));
}

test("every message the view builds exists in English and Vietnamese", () => {
  const [en, vi] = [keys("en"), keys("vi")];
  assert.deepEqual([...en].sort(), [...vi].sort());
  const built = [
    ...["strict", "normal", "loose"].flatMap((p) => [`similar-preset-${p}`, `similar-preset-${p}-hint`]),
    ...["identical", "similar"].flatMap((m) => [`similar-mode-${m}`, `similar-mode-${m}-hint`]),
    ...["scan", "collect", "hash", "compare"].map((p) => `similar-phase-${p}`),
    ...["small", "cloud", "reparse", "system", "protected", "app-dir", "unreadable", "changed", "too-large", "plain"].flatMap((r) => [`similar-skip-${r}`, `similar-skip-${r}-why`]),
    ...["pixels", "size", "place"].map((r) => `similar-best-${r}`),
    ...["recycle", "quarantine"].map((a) => `similar-confirm-${a}`),
    "activity-kind-similar",
    "reason-not-similar",
  ];
  for (const k of built) assert.ok(en.has(k), k);
  // and every literal key of the component
  const source = ["components/dupes/SimilarPhotos.svelte", "components/dupes/ModeSwitch.svelte", "similar/state.svelte.ts"]
    .map((f) => readFileSync(new URL(f, import.meta.url), "utf8"))
    .join("\n");
  for (const m of source.matchAll(/t\("(similar-[\w-]+)"/g)) assert.ok(en.has(m[1]), m[1]);
});
