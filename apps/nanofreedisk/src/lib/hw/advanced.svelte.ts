// Advanced sensors (PawnIO, ADR 0002) shared by Hardware → Sensors and the Home
// temperature card: status, one-click setup, starting the elevated helper and the
// "turn on automatically" setting.

import { errorText } from "../api";
import { t } from "../i18n.svelte";
import { app, saveSettingsSoon, toast } from "../state.svelte";
import { hwApi } from "./api";
import { advancedStep } from "./sensors";
import type { AdvancedStatus } from "./types";

export { advancedStep, type AdvancedStep } from "./sensors";

export const advanced = $state({
  status: null as AdvancedStatus | null,
  /** Setup, start or the no-prompt task in progress (Windows may show a prompt). */
  busy: null as null | "setup" | "start" | "task",
});

export async function refreshAdvanced(): Promise<void> {
  try {
    advanced.status = await hwApi.advancedStatus();
  } catch {
    advanced.status = null;
  }
}

function code(e: unknown): string {
  return e && typeof e === "object" && "code" in e ? String((e as { code: unknown }).code) : "";
}

/** Start the helper (UAC). A successful manual start turns the automatic start on. */
export async function startAdvanced(): Promise<void> {
  advanced.busy = "start";
  try {
    const s = await hwApi.advancedStart(false);
    advanced.status = s;
    if (s.unavailable) toast(t("hw-adv-unavailable", { reason: s.unavailable }), "error");
    else if (s.active && !app.settings.advancedAuto) {
      app.settings.advancedAuto = true;
      saveSettingsSoon();
    }
  } catch (e) {
    toast(code(e) === "uac-declined" ? t("hw-adv-declined") : t("hw-adv-failed", { message: errorText(e) }), "error");
  } finally {
    advanced.busy = null;
  }
}

/** Install PawnIO and/or fetch the official module, then start the helper. */
export async function setupAdvanced(): Promise<void> {
  advanced.busy = "setup";
  try {
    const s = await hwApi.pawnioSetup();
    advanced.status = s;
    if (s.restart && !s.pawnio?.running) {
      toast(t("hw-adv-restart"), "info");
      return;
    }
    if (advancedStep(s) === "start") {
      advanced.busy = null;
      await startAdvanced();
    }
  } catch (e) {
    const c = code(e);
    toast(
      c === "uac-declined"
        ? t("hw-adv-setup-declined")
        : c === "download-failed" || c === "download-mismatch"
          ? t("hw-adv-download-failed", { message: errorText(e) })
          : t("hw-adv-setup-failed", { message: errorText(e) }),
      "error",
    );
    await refreshAdvanced();
  } finally {
    advanced.busy = null;
  }
}

/** Called by the views that show CPU temperatures: once per app session the
 * backend may start the helper on its own when the setting is on. */
export async function autoStartAdvanced(): Promise<void> {
  for (let i = 0; i < 50 && !app.settingsLoaded; i++) await new Promise((r) => setTimeout(r, 100));
  if (!app.settings.advancedAuto || advanced.status?.active || advanced.busy) return;
  advanced.busy = "start";
  try {
    advanced.status = await hwApi.advancedStart(true);
  } catch (e) {
    if (code(e) !== "uac-declined") console.warn("advanced sensors", e);
    await refreshAdvanced();
  } finally {
    advanced.busy = null;
  }
}

export function setAdvancedAuto(on: boolean): void {
  app.settings.advancedAuto = on;
  saveSettingsSoon();
}

/** "Start without the Windows prompt" (ADR 0003): a scheduled task that starts the
 * helper's protected copy. The state lives in Windows, not in the settings. */
export async function setHelperTask(on: boolean): Promise<void> {
  advanced.busy = "task";
  try {
    advanced.status = await hwApi.helperTaskSet(on);
    toast(t(on ? "hw-adv-noprompt-on" : "hw-adv-noprompt-off"), "info");
  } catch (e) {
    toast(code(e) === "uac-declined" ? t("hw-adv-noprompt-declined") : t("hw-adv-noprompt-failed", { message: errorText(e) }), "error");
    await refreshAdvanced();
  } finally {
    advanced.busy = null;
  }
}
