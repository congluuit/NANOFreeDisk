// Typed bridge to the Startup view (src-tauri/src/commands/startup.rs).

import { call } from "../api";
import type { StartupEntry, StartupIcon } from "./types";

export const startupApi = {
  list: () => call<StartupEntry[]>("startup_list"),
  /** Turn this user's entry on or off (Task Manager's flag). All-users entries: `needs-admin`. */
  set: (id: string, enabled: boolean) => call<StartupEntry>("startup_set", { id, enabled }),
  icons: (ids: string[]) => call<Record<string, StartupIcon>>("startup_icons", { ids }),
  openTaskManager: () => call<void>("startup_open_task_manager"),
};
