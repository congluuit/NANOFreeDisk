// Drive health alerts (G5): keeps the backend watch in step with the setting and the
// UI language (notification and tray texts), and shows an in-app toast next to the
// system notification.

import { on } from "../api";
import { i18n, t } from "../i18n.svelte";
import { app, toast } from "../state.svelte";
import { healthApi } from "./api";

interface AlertEvent {
  kind: "status" | "hot" | "low-space";
  subject: string;
  status: "good" | "caution" | "bad" | "unknown" | null;
  value: number;
  limit: number | null;
}

let started = false;

/** Texts the backend fills in (`{subject}`, `{status}`, `{value}`, `{limit}`). */
function texts(): Record<string, string> {
  const m = { subject: "{subject}", status: "{status}", value: "{value}", limit: "{limit}" };
  return {
    "tray-open": t("health-tray-open"),
    "tray-quit": t("health-tray-quit"),
    "tray-status": t("health-tray-status", { status: "{status}" }),
    "tray-free": t("health-tray-free", { drive: "{drive}", size: "{size}", percent: "{percent}" }),
    "status-good": t("health-status-good"),
    "status-caution": t("health-status-caution"),
    "status-bad": t("health-status-bad"),
    "status-unknown": t("health-status-unknown"),
    "alert-status-title": t("health-alert-status-title", m),
    "alert-status-body": t("health-alert-status-body", m),
    "alert-hot-title": t("health-alert-hot-title", m),
    "alert-hot-body": t("health-alert-hot-body", m),
    "alert-low-title": t("health-alert-low-title", m),
    "alert-low-body": t("health-alert-low-body", m),
  };
}

/** Call once from App.svelte. */
export function startHealthWatch(): void {
  if (started) return;
  started = true;
  $effect.root(() => {
    $effect(() => {
      const enabled = app.settings.healthAlerts;
      // the Hardware Monitor's temperature alerts already cover hot drives
      const temperatures = !app.settings.tempAlerts;
      void i18n.locale;
      if (!app.settingsLoaded) return;
      healthApi.watchSet(enabled, temperatures, texts()).catch(() => {});
    });
  });
  void on<AlertEvent>("health-alert", (a) => {
    const subject = a.subject.replace(/\\$/, "");
    const text =
      a.kind === "status"
        ? t("health-alert-status-title", { subject, status: t(`health-status-${a.status ?? "unknown"}`), value: "", limit: "" })
        : a.kind === "hot"
          ? t("health-alert-hot-title", { subject, status: "", value: Math.round(a.value), limit: Math.round(a.limit ?? 0) })
          : t("health-alert-low-title", { subject, status: "", value: Math.round(a.value), limit: "" });
    toast(text, "error");
  });
}
