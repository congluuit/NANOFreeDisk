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
  PurgeResult,
  QuarantineInfo,
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
  /** Delete what the quarantine holds of operation `id` for good (all, or `items`). */
  purge: (id: string, items?: number[]) => call<PurgeResult>("activity_purge", { id, items: items ?? null }),
  quarantine: () => call<QuarantineInfo>("quarantine_summary"),
  emptyQuarantine: () => call<PurgeResult>("quarantine_empty"),
  revealQuarantine: (index: number) => call<void>("quarantine_reveal", { index }),
};
