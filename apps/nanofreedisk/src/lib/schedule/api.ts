// Typed bridge to automatic cleaning, the low space alert and autostart
// (src-tauri/src/commands/schedule.rs).

import { call } from "../api";
import type { OperationResult } from "../clean/types";
import type { Autostart, ScheduleConfig, SchedulePreview, ScheduleState } from "./types";

export const scheduleApi = {
  get: () => call<ScheduleState>("schedule_get"),
  /** Store the choices; `texts` are the notification texts in the UI language. */
  set: (config: ScheduleConfig, texts?: Record<string, string>) => call<ScheduleState>("schedule_set", { config, texts }),
  texts: (texts: Record<string, string>) => call<void>("schedule_texts", { texts }),
  /** What an automatic clean would take now (read-only); "Run now" cleans exactly this. */
  preview: () => call<SchedulePreview>("schedule_preview"),
  runNow: () => call<OperationResult>("schedule_run_now"),
  autostartSet: (enabled: boolean) => call<Autostart>("autostart_set", { enabled }),
};
