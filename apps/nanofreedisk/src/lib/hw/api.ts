// Typed bridge to the Hardware view's Rust commands (src-tauri/src/commands/hwmon.rs).

import { call } from "../api";
import type { HwInventory, SensorSnapshot } from "./types";

export const hwApi = {
  inventory: () => call<HwInventory>("hw_inventory"),
  exportInventory: (path: string) => call<void>("hw_export", { path }),
  sensors: () => call<SensorSnapshot>("sensors_snapshot"),
  resetMinMax: () => call<void>("sensors_reset_minmax"),
  history: (id: string, n: number) => call<number[]>("sensors_history", { id, n }),
};
