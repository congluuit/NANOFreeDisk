// Typed bridge to the Monitor's Rust commands (src-tauri/src/commands/monitor.rs).
// Read-only: no command here can end or change a process.

import { call } from "../api";
import type { MonitorHistory, MonitorTexts, MonitorView, Point, SpamConfig } from "./types";

export const monitorApi = {
  /** Polled about once a second by the view; keeps sampling on while it is open. */
  snapshot: () => call<MonitorView>("monitor_snapshot"),
  chart: (seconds: number) => call<Point[]>("monitor_chart", { seconds }),
  pause: (paused: boolean) => call<boolean>("monitor_pause", { paused }),
  /** File-level detail; may show UAC (elevated helper). */
  detail: (enabled: boolean) => call<MonitorView>("monitor_detail", { enabled }),
  watch: (path: string | null) => call<MonitorView>("monitor_watch", { path }),
  configure: (alerts: SpamConfig, texts: MonitorTexts, record: boolean) =>
    call<void>("monitor_configure", { alerts, texts, record }),
  history: () => call<MonitorHistory>("monitor_history"),
  /** Only paths the monitor reported. */
  reveal: (path: string) => call<void>("monitor_reveal", { path }),
};
