// Typed bridge to the Duplicates view (src-tauri/src/commands/dupes.rs).

import { call } from "../api";
import type { OperationResult } from "../clean/types";
import type { DupeAction, DupeReport, FindOptions, Selection } from "./types";

export const dupesApi = {
  /** Search `paths`, or the current Analyze scan when `paths` is empty. */
  find: (paths: string[], options: FindOptions) => call<DupeReport>("dupes_find", { paths, options }),
  cancel: () => call<void>("dupes_cancel"),
  report: () => call<DupeReport | null>("dupes_report"),
  select: (strategy: string, folders = false) => call<Selection[]>("dupes_select", { strategy, folders }),
  run: (selections: Selection[], action: DupeAction, folders = false) =>
    call<OperationResult>("dupes_run", { selections, action, folders }),
  reveal: (path: string) => call<void>("dupes_reveal", { path }),
  thumbnail: (path: string) => call<string | null>("dupes_thumbnail", { path }),
  export: (path: string, format: "json" | "csv") => call<void>("dupes_export", { path, format }),
  takeStartup: () => call<string | null>("take_startup_dupes"),
};
