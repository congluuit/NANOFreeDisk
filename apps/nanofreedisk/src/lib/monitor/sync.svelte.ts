// Log-spam alerts (PLAN §13.2 C8): keeps the backend in step with the settings and
// the UI language, and shows an in-app toast next to the system notification.

import { on } from "../api";
import { formatSize } from "../format";
import { i18n, intlLocale, t } from "../i18n.svelte";
import { app, toast } from "../state.svelte";
import { monitorApi } from "./api";
import type { Alert } from "./types";

let started = false;

/** Call once from App.svelte. */
export function startMonitorSync(): void {
  if (started) return;
  started = true;
  $effect.root(() => {
    $effect(() => {
      const alerts = {
        enabled: app.settings.monitorAlerts,
        mbps: Math.max(0.1, Number(app.settings.monitorMbps) || 5),
        minutes: Math.max(1, Math.round(Number(app.settings.monitorMinutes) || 5)),
      };
      const record = app.settings.history;
      void i18n.locale;
      if (!app.settingsLoaded) return;
      monitorApi
        .configure(
          alerts,
          {
            // the backend fills these markers in when an alert fires
            spamTitle: t("mon-alert-spam-title", { name: "{name}" }),
            spamBody: t("mon-alert-spam-body", { name: "{name}", rate: "{rate}", minutes: "{minutes}" }),
            logTitle: t("mon-alert-log-title", { name: "{name}" }),
            logBody: t("mon-alert-log-body", { name: "{name}", path: "{path}", size: "{size}" }),
          },
          record,
        )
        .catch(() => {});
    });
  });
  void on<Alert>("monitor-alert", (a) => toast(alertText(a), "error"));
}

/** One line describing an alert, in the UI language. */
export function alertText(a: Alert): string {
  const loc = intlLocale();
  if (a.kind === "log-spam") {
    return t("mon-alert-spam-toast", { name: a.name, rate: `${formatSize(a.rate, loc)}/s`, minutes: a.minutes });
  }
  return t("mon-alert-log-toast", { name: a.name, path: a.path, size: formatSize(a.size, loc) });
}
