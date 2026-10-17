// Advice of the loaded scan (Analyze → Suggestions): the backend computes it after
// every scan and deletion and says so with `advice-ready`; this keeps the summary
// and the user's ticks.

import { api, on } from "../api";
import { defaultPicks } from "../advice";
import { formatCount, formatSize } from "../format";
import { intlLocale, t } from "../i18n.svelte";
import type { SelItem } from "../selection";
import { afterDelete, app, expert, fail, toast } from "../state.svelte";
import type { AdviceSummary, Verdict } from "../types";

/** One line of the confirmation. */
export interface ConfirmRow {
  key: string;
  verdict: Verdict;
  label: string;
  location: string | null;
  bytes: number;
  impact: string;
}

/** What the confirmation is about: suggestion groups, one entry (`entry|<id>`) or
 * everything a folder holds (`within|<id>`). */
export interface ConfirmRequest {
  keys: string[];
  rows: ConfirmRow[];
  /** For the totals per level (and the 🟡 acknowledgement). */
  items: SelItem[];
  /** Entry ids that leave the tree on success. */
  touched: number[];
}

/** "Delete anyway": what the user overrules the app on. */
export interface OverrideRequest {
  ids: number[];
  rows: ConfirmRow[];
  /** Why the app kept it. */
  why: string;
  blocked: string | null;
  bytes: number;
}

export const advice = $state({
  summary: null as AdviceSummary | null,
  loading: false,
  /** Ticked group keys. */
  picks: [] as string[],
  /** Scan (`seq`) the ticks were made for: a new scan starts from the defaults. */
  picksSeq: -1,
  /** Confirmation dialog open. */
  confirm: null as ConfirmRequest | null,
  /** "Delete anyway" dialog open. */
  override: null as OverrideRequest | null,
  running: false,
});

let listening = false;

/** Reload the summary whenever the backend stores a new advice. */
export async function listenAdvice(): Promise<void> {
  if (listening) return;
  listening = true;
  await on<{ seq: number; enriched: boolean }>("advice-ready", () => {
    void loadAdvice();
    // tree badges and the details panel refetch
    app.version++;
  });
}

let seq = 0;
/** Group keys already shown for the current scan (a new one gets its default tick). */
let seen = new Set<string>();
export async function loadAdvice(): Promise<void> {
  if (!app.summary) {
    advice.summary = null;
    return;
  }
  const mine = ++seq;
  advice.loading = true;
  try {
    const s = await api.adviceSummary();
    if (mine !== seq) return;
    advice.summary = s;
    const groups = [...(s.suggestions?.groups ?? []), ...s.elsewhere];
    if (s.suggestions && s.seq !== advice.picksSeq) {
      advice.picks = defaultPicks(groups);
      advice.picksSeq = s.seq;
      seen = new Set(groups.map((g) => g.key));
    } else if (s.suggestions) {
      // the same scan judged again (git checked the agent worktrees, a clean, other
      // rules): keep the user's ticks, tick the safe groups seen for the first time
      const keys = new Set(groups.filter((g) => g.actionable).map((g) => g.key));
      const fresh = defaultPicks(groups).filter((k) => !seen.has(k));
      advice.picks = [...advice.picks.filter((k) => keys.has(k)), ...fresh];
      for (const g of groups) seen.add(g.key);
    }
  } catch {
    if (mine === seq) advice.summary = null;
  } finally {
    if (mine === seq) advice.loading = false;
  }
}

/** Clean what `req` names (`caution`: the user confirmed the 🟡 ones). */
export async function cleanAdvice(req: ConfirmRequest, caution: boolean): Promise<void> {
  const keys = req.keys;
  if (!keys.length || advice.running) return;
  advice.running = true;
  const touched = req.touched;
  try {
    const out = await api.adviceClean(keys, caution, expert());
    const loc = intlLocale();
    const op = out.operation;
    const recycled = out.recycled.filter((r) => r.status === "done");
    const freed = (op?.freed ?? 0) + recycled.reduce((s, r) => s + r.onDisk, 0);
    const count = (op?.done ?? 0) + (op?.partial ?? 0) + recycled.length;
    toast(t("advice-done", { size: formatSize(freed, loc), count: formatCount(count, loc) }), "ok");
    const problems = (op?.skipped ?? 0) + (op?.failed ?? 0) + out.recycled.filter((r) => r.status !== "done").length;
    if (problems) toast(t("advice-done-problems", { count: problems }), "info");
    if (out.skipped.length) toast(t("advice-skipped", { count: out.skipped.length }), "info");
    advice.picks = advice.picks.filter((k) => !keys.includes(k));
    await afterDelete([...touched, ...recycled.map((r) => r.id)]);
  } catch (e) {
    fail(e);
  } finally {
    advice.running = false;
    advice.confirm = null;
    void loadAdvice();
  }
}

/** "Delete anyway": move what the app kept out of the way (quarantine, or git for a
 * clean worktree) after the user acknowledged it. */
export async function overrideAdvice(req: OverrideRequest): Promise<void> {
  if (!req.ids.length || advice.running) return;
  advice.running = true;
  try {
    const out = await api.adviceOverride(req.ids, true);
    const loc = intlLocale();
    const op = out.operation;
    toast(t("advice-override-done", { size: formatSize(op?.freed ?? 0, loc), count: formatCount((op?.done ?? 0) + (op?.partial ?? 0), loc) }), "ok");
    const problems = (op?.skipped ?? 0) + (op?.failed ?? 0);
    if (problems) toast(t("advice-done-problems", { count: problems }), "info");
    if (out.skipped.length) toast(t("advice-override-skipped", { count: out.skipped.length }), "info");
    await afterDelete(req.ids);
  } catch (e) {
    fail(e);
  } finally {
    advice.running = false;
    advice.override = null;
    void loadAdvice();
  }
}
