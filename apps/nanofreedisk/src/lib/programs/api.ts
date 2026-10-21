// Typed bridge to the Programs view (src-tauri/src/commands/programs.rs).

import { call } from "../api";
import type { OperationResult, RestoreResult } from "../clean/types";
import type { JunkReport, LeftoverReport, Program, ProgramIcon, UninstallOutcome } from "./types";

export const programsApi = {
  list: (store: boolean) => call<Program[]>("programs_list", { store }),
  icons: (ids: string[]) => call<Record<string, ProgramIcon>>("programs_icons", { ids }),
  uninstall: (id: string, quiet: boolean) => call<UninstallOutcome>("programs_uninstall", { id, quiet }),
  stopWaiting: () => call<void>("programs_stop_waiting"),
  leftovers: (id: string, forced: boolean) => call<LeftoverReport>("programs_leftovers", { id, forced }),
  removeLeftovers: (id: string, picked: string[]) => call<OperationResult>("programs_leftovers_remove", { id, picked }),
  junkScan: () => call<JunkReport>("programs_junk_scan"),
  junkClean: (paths: string[]) => call<OperationResult>("programs_junk_clean", { paths }),
  reveal: (path: string) => call<void>("programs_reveal", { path }),
  restoreAdmin: (id: string, items?: number[]) =>
    call<RestoreResult[]>("activity_restore_admin", { id, items: items ?? null }),
};
