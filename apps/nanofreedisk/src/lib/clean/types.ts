// Shapes of the Clean / Activity commands (src-tauri/src/commands/clean.rs).

export type Level = "safe" | "caution" | "advanced" | "protected";

/** One rule of a pack as the last rule scan saw it (`nfd_rules::matcher::RuleSummary`). */
export interface RuleSummary {
  ruleId: string;
  packId: string;
  app: string;
  vendor: string;
  category: string;
  level: Level | null;
  impact: string;
  impactVi: string | null;
  requiresClosed: string[];
  /** Apps of `requiresClosed` running right now. */
  running: string[];
  bytes: number;
  files: number;
  candidates: number;
  skipped: number;
  refused: number;
  truncated: boolean;
}

export interface CleanPack {
  id: string;
  name: string;
  bytes: number;
}

export interface CleanScan {
  at: number;
  rules: RuleSummary[];
  packs: CleanPack[];
  safe: number;
  caution: number;
  advanced: number;
}

export interface Candidate {
  path: string;
  isDir: boolean;
  bytes: number;
  files: number;
  newest: number;
}

export interface CleanProgress {
  done: number;
  total: number;
  freed: number;
  path: string;
}

export interface Problem {
  path: string;
  status: "skipped" | "partial" | "failed";
  reason: string | null;
}

export interface OperationResult {
  id: string;
  freed: number;
  done: number;
  partial: number;
  skipped: number;
  failed: number;
  cancelled: boolean;
  problems: Problem[];
  journalError: string | null;
}

/** One entry of the Activity log (`nfd_history::OperationRow`). */
export interface OperationRow {
  id: string;
  kind: "clean" | "manual" | "tidy" | string;
  startedAt: number;
  finishedAt: number;
  items: number;
  done: number;
  bytes: number;
  freed: number;
  cancelled: boolean;
  state: "active" | "restored" | "partly-restored" | "purged" | string;
  restorable: number;
  /** Of those, the items the quarantine holds (files, or `.reg` backups of
   *  registry leftovers); the others are in the Recycle Bin. */
  quarantined: number;
  /** Bytes they keep on the disk until they are deleted for good. */
  held: number;
  /** When the quarantine deletes its files by itself (Unix seconds). */
  expiresAt: number | null;
}

export interface OperationItem {
  n: number;
  path: string;
  ruleId: string | null;
  mode: "recycle-bin" | "quarantine" | "permanent" | "hardlink" | "clone" | "registry";
  isDir: boolean;
  bytes: number;
  files: number;
  freed: number;
  status: string;
  reason: string | null;
  stored: string | null;
  restored: boolean;
  /** What a command ran and how to undo it (package stores, git worktrees). */
  message: string | null;
  /** Deleted for good from the quarantine. */
  purged: boolean;
}

/** Outcome of deleting quarantined items for good. */
export interface PurgeResult {
  items: number;
  bytes: number;
  /** `path: error` of items that stay in the quarantine. */
  failed: string[];
}

/** What the quarantine holds (`quarantine_summary`). */
export interface QuarantineInfo {
  items: number;
  bytes: number;
  operations: number;
  /** When the quarantine next deletes files by itself (Unix seconds). */
  nextExpiry: number | null;
  ttlDays: number;
  /** The quarantine folders on this machine. */
  roots: string[];
}

export interface ActivityDetail {
  operation: OperationRow;
  items: OperationItem[];
}

export interface RestoreResult {
  n: number;
  path: string;
  status: "restored" | "conflicts" | "missing" | "not-restorable" | "needs-admin" | "failed";
  message: string | null;
}

/** Result of one hand-picked entry (Analyze, Old & Large, Tidy up). */
export interface DeleteOutcome {
  id: number;
  status: "done" | "partial" | "declined" | "blocked" | "failed";
  reason: string | null;
  message: string | null;
  onDisk: number;
}

export type DeleteMode = "recycle-bin" | "quarantine";
