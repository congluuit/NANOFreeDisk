// Automatic cleaning and the low space alert (docs/SCHEDULE.md): keeps the
// backend's notification texts in the UI language, opens Quick clean from the
// tray's "Free up space" and tells the views when an automatic clean finished.
// The checks themselves run in the backend, also while the window is hidden.

import { on } from "../api";
import type { OperationResult } from "../clean/types";
import { formatSize } from "../format";
import { i18n, intlLocale, t } from "../i18n.svelte";
import { currentOs, hasFeature } from "../platform";
import { app, toast, type View } from "../state.svelte";
import { scheduleApi } from "./api";
import { freeUpView, noteTexts } from "./model";

/** Bumped when an automatic clean finished (the Settings section reloads). */
export const scheduleWatch = $state({ version: 0 });

let started = false;
const QUICK = "quick";

/** Open the view that frees space: Quick clean when the app has it, else Clean. */
export function openFreeUp(): void {
  const os = currentOs(app.info?.os);
  // the Quick clean view brings its own navigation label (built here, not a
  // literal key: the view may not exist in this build)
  const key = `nav-${QUICK}`;
  const hasQuick = t(key) !== key;
  app.view = freeUpView(hasQuick, hasFeature(os, "clean")) as View;
}

/** Call once at start. */
export function startScheduleWatch(): void {
  if (started) return;
  started = true;
  $effect.root(() => {
    $effect(() => {
      void i18n.locale;
      if (!app.settingsLoaded) return;
      scheduleApi.texts(noteTexts(t)).catch(() => {});
    });
  });
  void on<string>("schedule-open", () => openFreeUp());
  void on<OperationResult | null>("schedule-cleaned", (r) => {
    scheduleWatch.version++;
    app.version++;
    if (r && r.freed > 0) toast(t("schedule-done", { size: formatSize(r.freed, intlLocale()) }), "ok");
  });
}
