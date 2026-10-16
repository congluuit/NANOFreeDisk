// Typed bridge to the Hardware view's Rust commands (src-tauri/src/commands/hwmon.rs).

import { call } from "../api";
import type { HwInventory } from "./types";

export const hwApi = {
  inventory: () => call<HwInventory>("hw_inventory"),
  exportInventory: (path: string) => call<void>("hw_export", { path }),
};
