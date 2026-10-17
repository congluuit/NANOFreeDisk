// Typed bridge to the DevCache view (src-tauri/src/commands/devcache.rs).

import { call } from "../api";
import type { Candidate, OperationResult } from "../clean/types";
import type { AiPlan, AiReport, DevCacheReport, RepoScan, StoreInfo, StoreRun, Toolchain, UnknownFolder } from "./types";

export const devApi = {
  scan: (thresholds: Record<string, number>, rulesDir?: string) =>
    call<DevCacheReport>("devcache_scan", { thresholds, rulesDir: rulesDir ?? null }),
  items: (ruleId: string, limit: number) => call<Candidate[]>("devcache_items", { ruleId, limit }),
  run: (ruleIds: string[], expert: boolean, permanent: boolean) =>
    call<OperationResult>("devcache_run", { ruleIds, expert, permanent }),
  cancel: () => call<void>("devcache_cancel"),
  unknown: (minMb?: number) => call<UnknownFolder[]>("devcache_unknown", { minMb: minMb ?? null }),
  toolchains: () => call<Toolchain[]>("devcache_toolchains"),
  repos: (root: string, keepRecentDays: number, staleDays: number, gitStatus: boolean) =>
    call<RepoScan>("devcache_repos", { root, keepRecentDays, staleDays, gitStatus }),
  reposClean: (paths: string[]) => call<OperationResult>("devcache_repos_clean", { paths }),
  stores: () => call<StoreInfo[]>("devcache_stores"),
  storeRun: (id: string, commandLine: string) => call<StoreRun>("devcache_store_run", { id, commandLine }),
  reveal: (path: string) => call<void>("devcache_reveal", { path }),
  propose: (url: string) => call<void>("devcache_propose", { url }),
  notify: (title: string, body: string) => call<void>("devcache_notify", { title, body }),
  aiScan: (roots: string[], keepRecentDays: number, fixture?: string) =>
    call<AiReport>("ai_leftovers_scan", { roots, keepRecentDays, fixture: fixture ?? null }),
  aiPlan: (paths: string[]) => call<AiPlan>("ai_leftovers_plan", { paths }),
  aiClean: (paths: string[]) => call<OperationResult>("ai_leftovers_clean", { paths }),
};
