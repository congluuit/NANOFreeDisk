// Typed bridge to the Hardware view's Rust commands (src-tauri/src/commands/hwmon.rs).

import { call } from "../api";
import type { AdvancedStatus, EndMode, ThermalStatus, EndResult, HwInventory, SensorSnapshot, TaskDetails, TaskSnapshot, TaskVerdict } from "./types";

export const hwApi = {
  inventory: () => call<HwInventory>("hw_inventory"),
  exportInventory: (path: string) => call<void>("hw_export", { path }),
  sensors: () => call<SensorSnapshot>("sensors_snapshot"),
  resetMinMax: () => call<void>("sensors_reset_minmax"),
  history: (id: string, n: number) => call<number[]>("sensors_history", { id, n }),
  tasks: () => call<TaskSnapshot>("tasks_snapshot"),
  verdict: (pid: number, createTime: string) => call<TaskVerdict>("task_verdict", { pid, createTime }),
  details: (pid: number, createTime: string) => call<TaskDetails>("task_details", { pid, createTime }),
  /** Only after the user confirmed in EndTaskDialog. */
  end: (pid: number, createTime: string, mode: EndMode, confirmed: boolean) =>
    call<EndResult[]>("task_end", { pid, createTime, mode, confirmed }),
  reveal: (pid: number, createTime: string) => call<void>("task_reveal", { pid, createTime }),
  openTaskManager: () => call<void>("open_task_manager"),
  advancedStatus: () => call<AdvancedStatus>("pawnio_status"),
  /** Shows UAC; adds PawnIO sensors from the elevated helper. */
  advancedStart: () => call<AdvancedStatus>("advanced_sensors_start"),
  pawnioSite: () => call<void>("pawnio_open_site"),
  thermalSet: (enabled: boolean, texts: { title: string; body: string }) =>
    call<ThermalStatus>("thermal_set", { enabled, texts }),
  thermalStatus: () => call<ThermalStatus>("thermal_status"),
};
