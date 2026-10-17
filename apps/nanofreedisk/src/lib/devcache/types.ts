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

export type DevTab = "cache" | "ai" | "toolchains" | "repos" | "stores" | "unknown";

// ── AI leftovers (`nfd_devtools::ai_leftovers`, docs/AI_LEFTOVERS.md) ──

export interface SessionRef {
  id: string;
  tool: string;
  title: string | null;
  lastActive: number;
  /** Still open (not archived) in the Claude app. */
  open: boolean;
  /** Active within the keep window. */
  recent: boolean;
}

export interface IgnoredEntry extends Totals {
  rel: string;
  isDir: boolean;
  mtime: number;
}

export interface WorktreeInfo {
  repo: string;
  branch: string | null;
  head: string | null;
  defaultBranch: string | null;
  merged: boolean;
  pushed: boolean;
  changes: number;
  locked: boolean;
  registered: boolean;
  prunable: boolean;
  ignored: IgnoredEntry[];
  removable: boolean;
  artifacts: Artifact[];
  command: string | null;
}

export type AiKind = "worktree" | "session-temp" | "folder" | "browser-revision";
export type AiKeep = "session-open" | "session-recent" | "recent-write";

export interface AiItem extends Totals {
  path: string;
  entry: string;
  tool: string;
  kind: AiKind;
  scope: "project" | "temp" | "global";
  project: string | null;
  rel: string;
  level: Level;
  mtime: number;
  session: SessionRef | null;
  keep: AiKeep | null;
  selected: boolean;
  blocked: string | null;
  impact: string;
  impactVi: string;
  worktree: WorktreeInfo | null;
}

export interface AiKept {
  path: string;
  tool: string;
  project: string | null;
  what: string;
}

export interface AiReport {
  items: AiItem[];
  kept: AiKept[];
  projects: string[];
  sessions: number;
  git: boolean;
  keepRecentDays: number;
  selectedBytes: number;
  totalBytes: number;
  cancelled: boolean;
}

export interface AiPlan {
  folders: number;
  bytes: number;
  commands: string[];
}
