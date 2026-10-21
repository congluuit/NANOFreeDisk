// Shapes of the Programs commands (src-tauri/src/commands/programs.rs, crates/nfd-apps).

import type { Level } from "../clean/types";

export type Scope = "user" | "machine";
export type Source = "win32" | "msi" | "store";

export interface Command {
  exe: string;
  args: string;
  raw: string;
}

export interface Program {
  /** `hklm:<key>`, `hklm32:<key>`, `hkcu:<key>`, `appx:<package>`. */
  id: string;
  name: string;
  publisher: string | null;
  version: string | null;
  source: Source;
  scope: Scope;
  arch: string | null;
  installLocation: string | null;
  uninstall: Command | null;
  quietUninstall: string | null;
  icon: string | null;
  installedAt: number | null;
  size: number;
  sizeMeasured: boolean;
  productCode: string | null;
  key: string | null;
  package: { fullName: string; family: string } | null;
  url: string | null;
  /** `runtime`, `driver`, `windows`. */
  caution: string | null;
  /** `self`, `webview2`, `no-remove`, `no-uninstaller`, `non-removable`. */
  blocked: string | null;
  /** The uninstaller is gone: only the leftovers can be removed. */
  broken: boolean;
}

export type LeftoverKind = "folder" | "file" | "shortcut" | "registry-key" | "registry-value" | "service";

export interface Leftover {
  id: string;
  kind: LeftoverKind;
  path: string;
  value: string | null;
  scope: Scope;
  level: Level;
  /** `install-dir`, `same-name`, `vendor-folder`, `weak-name`, `shortcut`,
   *  `start-menu-folder`, `arp-entry`, `software-key`, `vendor-key`, `app-path`,
   *  `run-entry`, `service`, `store-data`. */
  reason: string;
  detail: string | null;
  /** Another installed program the name also fits. */
  also: string | null;
  bytes: number;
  files: number;
  mtime: number;
  selected: boolean;
  blocked: string | null;
  /** Big enough to hold the user's own files: never pre-selected. */
  large: boolean;
}

export type LeftoverMode = "after-uninstall" | "broken" | "forced";

export interface LeftoverReport {
  program: Program;
  mode: LeftoverMode;
  items: Leftover[];
  selectedBytes: number;
  totalBytes: number;
}

export type JunkKind = "cache" | "crash" | "log" | "temp" | "update";

export interface JunkItem {
  path: string;
  kind: JunkKind;
  /** `chromium-cache`, `service-worker`, `electron-logs`, `crashpad`, `crash-dumps`,
   *  `crashes`, `dump-file`, `logs`, `temp`, `old-version`, `update-package`,
   *  `update-temp`, `pending-update`. */
  what: string;
  level: Level;
  isDir: boolean;
  bytes: number;
  files: number;
  newest: number;
  size: number;
  mtime: number;
  selected: boolean;
  blocked: string | null;
}

export interface JunkGroup {
  key: string;
  program: string | null;
  name: string;
  publisher: string | null;
  folder: string;
  requiresClosed: string[];
  running: boolean;
  items: JunkItem[];
  bytes: number;
}

export interface JunkReport {
  groups: JunkGroup[];
  totalBytes: number;
  selectedBytes: number;
  cancelled: boolean;
}

export type UninstallStatus = "removed" | "still-installed" | "declined" | "failed" | "broken";

export interface UninstallOutcome {
  id: string;
  status: UninstallStatus;
  waited: "done" | "stopped" | "timed-out" | null;
  message: string | null;
}

export interface UninstallProgress {
  id: string;
  phase: "running" | "waiting";
  alive: number;
}
