// Clean view state: the last rule scan (shared with Home's "free X GB safely" button
// and the Health Score), the running clean and its result.

import { on } from "../api";
import { formatSize } from "../format";
import { i18n, intlLocale, t } from "../i18n.svelte";
import { app, fail, toast } from "../state.svelte";
import { cleanApi } from "./api";
import type { CleanProgress, CleanScan, Level, OperationResult, RuleSummary } from "./types";

export const clean = $state({
  scan: null as CleanScan | null,
  scanning: false,
  /** Rules ticked in the Clean view (rule ids). */
  picked: [] as string[],
  /** Confirmation dialog open for these rule ids. */
  confirm: null as null | string[],
  running: null as CleanProgress | null,
  last: null as OperationResult | null,
});

/** Rules that can be cleaned in the current mode (Advanced in Expert only). */
export function cleanable(r: RuleSummary, expert: boolean): boolean {
  return r.candidates > 0 && (r.level === "safe" || r.level === "caution" || (expert && r.level === "advanced"));
}

/** Default ticks: every 🟢 Safe rule with something to clean. */
export function defaultPicks(scan: CleanScan): string[] {
  return scan.rules.filter((r) => r.level === "safe" && r.candidates > 0).map((r) => r.ruleId);
}

/** The rule's "if deleted…" text in the UI language. */
export function impactText(r: RuleSummary): string {
  return i18n.locale === "vi" && r.impactVi ? r.impactVi : r.impact;
}

/** Why an item was skipped or failed (gate codes, `in-use`, I/O error kinds). */
export function reasonText(code: string | null): string {
  if (!code) return "";
  const key = `reason-${code}`;
  const text = t(key, { template: "" });
  return text === key ? t("reason-unknown", { code }) : text;
}

export function levelOf(r: RuleSummary): Level {
  return r.level ?? "protected";
}

let scanning: Promise<void> | null = null;

/** `scripts/gui-smoke.mjs` points the scan at a rule pack that only matches its
 * fixture (the packs are validated by the backend and still pass the SafetyGate). */
const testRulesDir = (): string | undefined => (globalThis as { __nfdRulesDir?: string }).__nfdRulesDir;

/** Match the rule packs (read-only). Concurrent calls share one scan. */
export function scanJunk(): Promise<void> {
  if (scanning) return scanning;
  clean.scanning = true;
  scanning = cleanApi
    .scan(testRulesDir())
    .then((s) => {
      clean.scan = s;
      clean.picked = defaultPicks(s);
    })
    .catch(fail)
    .finally(() => {
      clean.scanning = false;
      scanning = null;
    });
  return scanning;
}

/** Bytes the given rules would free. */
export function bytesOf(ruleIds: string[]): number {
  return clean.scan?.rules.filter((r) => ruleIds.includes(r.ruleId)).reduce((s, r) => s + r.bytes, 0) ?? 0;
}

let progressOff: (() => void) | null = null;

/** Clean `ruleIds` (already confirmed). */
export async function runClean(ruleIds: string[], expert: boolean, permanent: boolean): Promise<void> {
  if (clean.running) return;
  clean.running = { done: 0, total: 0, freed: 0, path: "" };
  clean.last = null;
  try {
    progressOff ??= await on<CleanProgress>("clean-progress", (p) => {
      if (clean.running) clean.running = p;
    });
    const result = await cleanApi.run(ruleIds, expert, permanent);
    clean.last = result;
    const loc = intlLocale();
    toast(
      t("clean-result", { size: formatSize(result.freed, loc), done: result.done + result.partial, skipped: result.skipped + result.failed }),
      result.failed ? "error" : "ok",
    );
    if (result.journalError) toast(t("clean-journal-failed", { error: result.journalError }), "error");
  } catch (e) {
    fail(e);
  } finally {
    clean.running = null;
    clean.scan = null;
    app.version++;
    void scanJunk();
  }
}
