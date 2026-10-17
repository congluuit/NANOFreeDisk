// Typed bridge to the per-website browser cache commands
// (src-tauri/src/commands/browsers.rs).

import { call } from "../api";
import type { OperationResult } from "../clean/types";
import type { SitePlan, SitesReport, SiteTarget } from "./types";

export const browsersApi = {
  sites: (fixture?: string) => call<SitesReport>("browsers_sites", { fixture: fixture ?? null }),
  plan: (targets: SiteTarget[], fixture?: string) => call<SitePlan>("browsers_plan", { targets, fixture: fixture ?? null }),
  clean: (targets: SiteTarget[], fixture?: string) => call<OperationResult>("browsers_clean", { targets, fixture: fixture ?? null }),
  cancel: () => call<void>("browsers_cancel"),
};
