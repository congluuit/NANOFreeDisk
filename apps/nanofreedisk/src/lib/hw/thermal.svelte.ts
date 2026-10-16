// Temperature alerts (slice 6): keeps the backend watcher in step with the setting
// and the UI language, and shows an in-app toast next to the system notification.

import { on } from "../api";
import { i18n, t } from "../i18n.svelte";
import { app, toast } from "../state.svelte";
import { hwApi } from "./api";

interface AlertEvent {
  part: "cpu" | "gpu" | "disk";
  device: string;
  value: number;
  limit: number;
}

let started = false;

/** Call once from App.svelte. */
export function startThermalSync(): void {
  if (started) return;
  started = true;
  $effect.root(() => {
    $effect(() => {
      const enabled = app.settings.tempAlerts;
      void i18n.locale;
      if (!app.settingsLoaded) return;
      hwApi
        .thermalSet(enabled, {
          title: t("hw-alert-title", { device: "{device}" }),
          // the backend fills these markers in when an alert fires
          body: t("hw-alert-body", { device: "{device}", value: "{value}", limit: "{limit}" }),
        })
        .catch(() => {});
    });
  });
  void on<AlertEvent>("temp-alert", (a) => {
    toast(
      t("hw-alert-toast", { part: t(`hw-part-${a.part}`), device: a.device, value: Math.round(a.value), limit: a.limit }),
      "error",
    );
  });
}
