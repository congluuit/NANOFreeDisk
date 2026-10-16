// Shapes of the DevCache commands (src-tauri/src/commands/devcache.rs, crate
// nfd-devtools).

import type { Level, RuleSummary } from "../clean/types";

/** One rule with the newest write of what it found (`nfd_devtools::devcache::DevRule`). */
export interface DevRule extends RuleSummary {
  newest: number;
}

export interface AppGroup {
  app: string;
  bytes: number;
  files: number;
  safeBytes: number;
  newest: number;
  running: string[];
  rules: DevRule[];
  /** 🔒 What the app deliberately keeps (🔴 Protected rules). */
  kept: DevRule[];
}

export interface VendorGroup {
  vendor: string;
  bytes: number;
  files: number;
  safeBytes: number;
  cautionBytes: number;
  advancedBytes: number;
  newest: number;
  running: string[];
  threshold: number | null;
  overThreshold: boolean;
  apps: AppGroup[];
}

export interface DevCacheReport {
  vendors: VendorGroup[];
  safe: number;
  caution: number;
  advanced: number;
  kept: number;
}

/** Folder totals (`nfd_devtools::fsutil::Totals`), flattened into the rows below. */
export interface Totals {
  bytes: number;
  files: number;
  newest: number;
  truncated: boolean;
}

export interface UnknownFolder extends Totals {
  path: string;
  name: string;
  root: string;
  template: string;
  level: Level;
  signals: string[];
  proposeUrl: string;
}

export type Guide = "windows-apps" | "unity-hub" | "epic-launcher" | "visual-studio-installer" | "ide" | "compact-vhd";

export interface Toolchain extends Totals {
  tool: string;
  family: string;
  name: string;
  path: string;
  default: boolean;
  command: string | null;
  guide: Guide | null;
}

export interface Artifact extends Totals {
  path: string;
  rel: string;
  kind: string;
  level: Level;
  mtime: number;
  selected: boolean;
  /** SafetyGate code, `tracked` or `not-ignored`. */
  blocked: string | null;
}

export interface Repo {
  path: string;
  name: string;
  git: boolean;
  kinds: string[];
  gitBytes: number;
  lastCommit: number | null;
  lastCommitSubject: string | null;
  lastActivity: number;
  dirty: boolean | null;
  recent: boolean;
  stale: boolean;
  artifacts: Artifact[];
  artifactBytes: number;
}

export interface RepoScan {
  root: string;
  repos: Repo[];
  git: boolean;
  folders: number;
  cancelled: boolean;
}

export interface StoreInfo extends Totals {
  id: string;
  tool: string;
  program: string | null;
  args: string[];
  commandLine: string;
  stores: string[];
  undo: "redownload" | "none";
  level: Level;
  runnable: boolean;
}

export interface RunOutput {
  code: number | null;
  stdout: string;
  stderr: string;
  timedOut: boolean;
  cancelled: boolean;
  millis: number;
}

export interface StoreRun {
  id: string;
  opId: string;
  before: number;
  after: number;
  freed: number;
  output: RunOutput;
  journalError: string | null;
}

export type DevTab = "cache" | "toolchains" | "repos" | "stores" | "unknown";
