// Typed bridge to the Recycle Bin card (src-tauri/src/commands/recyclebin.rs).

import { call } from "../api";
import type { BinStatus, EmptyOutcome } from "./types";

export const recycleBinApi = {
  status: () => call<BinStatus>("recyclebin_status"),
  /** Empty the bin of drive `root` (`C:\`), or of every drive. Permanent. */
  empty: (root?: string) => call<EmptyOutcome>("recyclebin_empty", { root: root ?? null }),
};
