// Typed bridge to the Clean view, the Activity log and every delete
// (src-tauri/src/commands/clean.rs).

import { call } from "../api";
import type {
  ActivityDetail,
  Candidate,
  CleanScan,
  DeleteMode,
  DeleteOutcome,
  OperationResult,
  OperationRow,
  RestoreResult,
} from "./types";

export const cleanApi = {
  scan: (rulesDir?: string) => call<CleanScan>("clean_scan", { rulesDir: rulesDir ?? null }),
  items: (ruleId: string, limit: number) => call<Candidate[]>("clean_items", { ruleId, limit }),
  run: (ruleIds: string[], expert: boolean, permanent: boolean) =>
    call<OperationResult>("clean_run", { ruleIds, expert, permanent }),
  cancel: () => call<void>("clean_cancel"),
  deleteEntries: (ids: number[], mode: DeleteMode, kind: "manual" | "tidy") =>
    call<DeleteOutcome[]>("delete_entries", { ids, mode, kind }),
  activity: (limit: number) => call<OperationRow[]>("activity_list", { limit }),
  activityItems: (id: string) => call<ActivityDetail>("activity_items", { id }),
  restore: (id: string, items?: number[]) =>
    call<RestoreResult[]>("activity_restore", { id, items: items ?? null }),
  purge: (id: string) => call<void>("activity_purge", { id }),
};
