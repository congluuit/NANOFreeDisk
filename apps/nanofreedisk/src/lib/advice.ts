// Analyze → Suggestions: pure helpers over the advice the backend computes
// (nfd-advice) — verdict colors and icons, section split, selection items, titles.
// Tested in advice.test.ts. Nothing here decides what may be deleted: the backend
// only acts on groups it reports as actionable, and every action still goes Planner
// → SafetyGate → Executor.

import type { SelItem } from "./selection";
import type { AdviceGroup, AdviceReason, BiText, ByVerdict, NodeAdvice, Verdict } from "./types";

export const VERDICTS: Verdict[] = ["safe", "caution", "guide", "keep", "unknown"];

/** Theme token of each verdict (theme.ts colors, so custom themes follow). */
export const VERDICT_TOKEN: Record<Verdict, string> = {
  safe: "--risk-safe",
  caution: "--risk-caution",
  guide: "--risk-advanced",
  // "better keep" is the app's suggestion (it may be wrong); red is kept for hard
  // protections ([PROTECTED_TOKEN])
  keep: "--tm-3",
  unknown: "--tm-other",
};

/** Hard protections (SafetyGate, conversation history): never deleted. */
export const PROTECTED_TOKEN = "--risk-protected";

export const VERDICT_ICON: Record<Verdict, string> = {
  safe: "check",
  caution: "alert",
  guide: "zoom",
  keep: "shield",
  unknown: "search",
};

/** Verdict of a treemap code (`Verdict::code` in Rust; 255 = none). */
export function verdictOfCode(code: number | undefined): Verdict | null {
  return code != null && code < VERDICTS.length ? VERDICTS[code] : null;
}

/** Display names of build-output kinds (proper nouns: not translated). */
export const KIND_NAMES: Record<string, string> = {
  rust: "Rust",
  node: "Node.js",
  python: "Python",
  dotnet: ".NET",
  cmake: "CMake",
  gradle: "Gradle",
  maven: "Maven",
  flutter: "Flutter",
  unity: "Unity",
  unreal: "Unreal Engine",
  godot: "Godot",
  terraform: "Terraform",
  ruby: "Ruby",
};

/** A text of the backend in the UI language. */
export function bi(text: BiText | null | undefined, lang: string): string {
  if (!text) return "";
  return lang === "vi" ? text.vi || text.en : text.en;
}

/** Fluent key and arguments of a group / unit / part title. */
export function titleOf(
  x: { reason: AdviceReason; app: string | null; source: string | null; title: BiText | null },
  lang: string,
): { key: string; args: Record<string, string> } {
  const kind = x.source ? (KIND_NAMES[x.source] ?? x.source) : "";
  return {
    key: `advice-reason-${x.reason}`,
    args: {
      app: x.app || x.source || "",
      kind,
      title: bi(x.title, lang) || x.source || "",
    },
  };
}

/** Why-not-now codes with their own message (`advice-blocked-<code>`). */
export const KNOWN_BLOCKED = [
  "git-check",
  "unverified",
  "session-live",
  "session-open",
  "dirty",
  "not-merged",
  "not-removable",
  "newest",
  "contains-history",
  "tracked",
];

/** Fluent key of a blocked code. */
export function blockedKey(code: string): string {
  return KNOWN_BLOCKED.includes(code) ? `advice-blocked-${code}` : "advice-blocked-other";
}

export type Section = "safe" | "caution" | "guide" | "keep" | "elsewhere";
export const SECTIONS: Section[] = ["safe", "caution", "guide", "keep", "elsewhere"];

/** Groups per section of the panel. "Big but keep" holds what must stay and the
 * user's own media (unknown, never suggested). */
export function sections(groups: AdviceGroup[], elsewhere: AdviceGroup[]): Record<Section, AdviceGroup[]> {
  const out: Record<Section, AdviceGroup[]> = { safe: [], caution: [], guide: [], keep: [], elsewhere: [] };
  for (const g of groups) {
    if (g.verdict === "safe" || g.verdict === "caution" || g.verdict === "guide") out[g.verdict].push(g);
    else if (g.verdict === "keep" || g.reason === "personal") out.keep.push(g);
  }
  out.keep.sort((a, b) => b.bytes - a.bytes);
  out.elsewhere = elsewhere.filter((g) => g.verdict === "safe" || g.verdict === "caution");
  return out;
}

/** Selection items of the groups an action can run on (others are not listed). */
export function selItems(groups: AdviceGroup[]): SelItem[] {
  return groups
    .filter((g) => g.verdict === "safe" || g.verdict === "caution")
    .filter((g) => g.action === "clean" || g.action === "quarantine" || g.action === "ai-leftovers" || g.action === "recycle-bin")
    .map((g) => ({
      id: g.key,
      bytes: g.bytes,
      files: g.files,
      level: g.level === "advanced" ? "advanced" : g.verdict === "safe" ? "safe" : "caution",
      group: g.elsewhere ? "elsewhere" : g.verdict,
      locked: !g.actionable,
    }));
}

/** What is ticked for the user: 🟢 groups the backend pre-selects. */
export function defaultPicks(groups: AdviceGroup[]): string[] {
  return groups.filter((g) => g.selected && g.actionable && g.verdict === "safe").map((g) => g.key);
}

/** Bar segments of a total (zero-sized verdicts left out). */
export function segments(t: ByVerdict | null | undefined): { verdict: Verdict; bytes: number; share: number }[] {
  if (!t) return [];
  const total = VERDICTS.reduce((s, v) => s + t[v], 0);
  if (!total) return [];
  return VERDICTS.filter((v) => t[v] > 0).map((v) => ({ verdict: v, bytes: t[v], share: t[v] / total }));
}

/** What the tree's badge shows: the unit's verdict (and whether it is a hard
 * protection), or (a mixed folder) "can free X". */
export function badgeOf(a: NodeAdvice | undefined): { verdict: Verdict | null; partly: boolean; hard: boolean } {
  if (!a) return { verdict: null, partly: false, hard: false };
  if (a.verdict) return { verdict: a.verdict, partly: false, hard: a.hard };
  if (a.reclaim > 0) return { verdict: null, partly: true, hard: false };
  const only = VERDICTS.filter((v) => a.inside[v] > 0);
  return { verdict: only.length === 1 ? only[0] : null, partly: false, hard: false };
}

/** Fluent key of a "Delete anyway" refusal (`advice-hard-<code>`). */
export function hardKey(code: string): string {
  return ["hard", "inside-hard", "contains-hard", "root"].includes(code) ? `advice-hard-${code}` : "advice-hard-other";
}

/** Where a non-runnable action leads: a view (and tab) of the app. */
export function actionTarget(action: string): { view: "devcache" | "duplicates" | null; tab?: string } {
  if (action === "store") return { view: "devcache", tab: "stores" };
  if (action === "duplicates") return { view: "duplicates" };
  return { view: null };
}
