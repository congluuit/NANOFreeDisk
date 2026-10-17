// Per-website browser cache panel state: the last read of the caches, the ticked
// sites, the confirmation and the running clean.

import { on } from "../api";
import type { CleanProgress, OperationResult } from "../clean/types";
import { formatSize } from "../format";
import { intlLocale, t } from "../i18n.svelte";
import { app, fail, toast } from "../state.svelte";
import { browsersApi } from "./api";
import { targetsOf } from "./sites";
import type { SitePlan, SitesReport } from "./types";

export const br = $state({
  report: null as SitesReport | null,
  loading: false,
  /** Site ids (`browser|profile|site`). */
  picked: [] as string[],
  query: "",
  /** Confirmation open with the plan of these ids. */
  confirm: null as null | { ids: string[]; plan: SitePlan },
  running: null as CleanProgress | null,
  last: null as OperationResult | null,
});

/** `scripts/gui-smoke.mjs` points the panel at a fixture laid out as
 *  `<dir>/Local`, `<dir>/Roaming` (never the real profiles). */
const fixture = (): string | undefined => (globalThis as { __nfdBrowserFixture?: string }).__nfdBrowserFixture;

let loading: Promise<void> | null = null;

/** Read the browser caches (read-only). Concurrent calls share one read. */
export function loadSites(): Promise<void> {
  if (loading) return loading;
  br.loading = true;
  loading = browsersApi
    .sites(fixture())
    .then((r) => {
      br.report = r;
      br.picked = [];
    })
    .catch(fail)
    .finally(() => {
      br.loading = false;
      loading = null;
    });
  return loading;
}

/** Open the confirmation for `ids` with what the backend would remove. */
export async function confirmSites(ids: string[]): Promise<void> {
  if (!ids.length) return;
  try {
    br.confirm = { ids, plan: await browsersApi.plan(targetsOf(ids), fixture()) };
  } catch (e) {
    fail(e);
  }
}

let progressOff: (() => void) | null = null;

/** Clean the confirmed sites. */
export async function cleanSites(): Promise<void> {
  const c = br.confirm;
  if (!c || br.running) return;
  br.confirm = null;
  br.running = { done: 0, total: 0, freed: 0, path: "" };
  br.last = null;
  try {
    progressOff ??= await on<CleanProgress>("clean-progress", (p) => {
      if (br.running) br.running = p;
    });
    const r = await browsersApi.clean(targetsOf(c.ids), fixture());
    br.last = r;
    toast(t("br-done", { size: formatSize(r.freed, intlLocale()), done: r.done + r.partial }), r.failed ? "error" : "ok");
    if (r.journalError) toast(t("clean-journal-failed", { error: r.journalError }), "error");
  } catch (e) {
    fail(e);
  } finally {
    br.running = null;
    app.version++;
    void loadSites();
  }
}
