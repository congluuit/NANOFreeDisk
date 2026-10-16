// Typed bridge to the Health view (src-tauri/src/commands/diskhealth.rs).

import { call } from "../api";
import type { HealthReport } from "./types";

export const healthApi = {
  /** The last report, or a fresh read when `refresh` (or none yet). */
  report: (refresh: boolean) => call<HealthReport>("health_report", { refresh }),
  /** Shows UAC once; adds SMART thresholds, USB bridges and the dirty bit. */
  elevated: () => call<HealthReport>("health_elevated"),
  exportJson: (path: string, serials: boolean) => call<void>("health_export", { path, serials }),
  watchSet: (enabled: boolean, texts: Record<string, string>) => call<void>("health_watch_set", { enabled, texts }),
};
