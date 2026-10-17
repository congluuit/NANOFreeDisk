// Shapes returned by the Rust commands in `src-tauri/src/commands.rs` / `session.rs`.

export type EntryKind = "file" | "dir" | "symlink" | "junction" | "mount_point" | "other";

export interface AppInfo {
  version: string;
  os: string;
  accent: string | null;
}

export interface Volume {
  root: string;
  label: string;
  fs: string;
  kind: "fixed" | "removable" | "network" | "optical" | "ramdisk" | "unknown";
  total: number;
  available: number;
  free: number;
  ready: boolean;
  system: boolean;
}

export interface Progress {
  scan: number;
  files: number;
  dirs: number;
  onDisk: number;
  errors: number;
  elapsedMs: number;
}

export interface Finished {
  scan: number;
  ok: boolean;
  cancelled: boolean;
  error: string | null;
  /** History snapshot written for this scan. */
  snapshot: number | null;
  historyError: string | null;
}

export interface ScanStats {
  files: number;
  dirs: number;
  symlinks: number;
  junctions: number;
  mount_points: number;
  other: number;
  hardlink_dups: number;
  logical: number;
  on_disk: number;
  errors: number;
  approximate: number;
}

export interface Summary {
  root: string;
  rootId: number;
  stats: ScanStats;
  elapsedMs: number;
  scannedAt: number;
  issues: number;
  mode: "standard" | "fast";
  volumeTotal: number;
  volumeFree: number;
}

export interface ScanIssue {
  path: string;
  kind: "access_denied" | "not_found" | "other";
  message: string;
}

export interface Node {
  id: number;
  name: string;
  kind: EntryKind;
  logical: number;
  onDisk: number;
  files: number;
  mtime: number;
  attrs: string[];
  hasChildren: boolean;
  hardlinkDup: boolean;
  /** Verdict and reclaimable bytes, once the advice of the scan is computed. */
  advice?: NodeAdvice;
}

export interface Located extends Node {
  path: string;
}

export interface Crumb {
  id: number;
  name: string;
}

export interface Detail extends Located {
  ancestors: Crumb[];
  share: number;
}

/** Kinds: 0 file, 1 dir, 2 link, 3 "small items". Parents precede children. */
export interface Slice {
  ids: number[];
  parents: number[];
  sizes: number[];
  kinds: number[];
  names: string[];
  depths: number[];
  truncated: boolean;
  /** Verdict code per node (0 safe … 4 unknown, 255 none); empty before the advice. */
  verdicts?: number[];
}

export const REST_ID = 4294967295;

export interface ExtRow {
  ext: string;
  files: number;
  onDisk: number;
  logical: number;
}

export interface Filter {
  name?: string | null;
  exts?: string[];
  min_size?: number | null;
  max_size?: number | null;
  modified_after?: number | null;
  modified_before?: number | null;
  kind?: "any" | "files" | "dirs";
  attrs_all?: number;
  attrs_none?: number;
}

export interface SearchRows {
  rows: Located[];
  matched: number;
  matchedOnDisk: number;
}

/** Thresholds of the Old & Large view (`nfd_core::insights::OldLargeOptions`). */
export interface OldLargeOptions {
  large_min: number;
  stale_min: number;
  stale_before: number;
  installer_before: number;
  limit: number;
}

export interface OldLargeRows {
  large: SearchRows;
  stale: SearchRows;
  installers: SearchRows;
  distinctFiles: number;
  distinctOnDisk: number;
}

export interface EmptyRow extends Located {
  /** Empty sub-folders inside it. */
  nested: number;
}

export interface EmptyRows {
  rows: EmptyRow[];
  matched: number;
  totalDirs: number;
}

export type ShortcutStatus = "ok" | "missing" | "drive_missing" | "unchecked";

export interface ShortcutRow extends Located {
  target: string;
  status: ShortcutStatus;
}

export interface ShortcutRows {
  rows: ShortcutRow[];
  checked: number;
  unchecked: number;
  unreadable: number;
}

/** A stored scan (`nfd_history::Snapshot`). */
export interface Snapshot {
  id: number;
  root: string;
  volume: string;
  startedAt: number;
  elapsedMs: number;
  files: number;
  dirs: number;
  onDisk: number;
  volumeTotal: number;
  volumeFree: number;
  issues: number;
  dirFloor: number;
  fileFloor: number;
}

export interface DirChange {
  /** Relative to the scanned folder. */
  path: string;
  depth: number;
  before: number;
  after: number;
  delta: number;
  filesDelta: number;
}

export interface FileChange {
  path: string;
  before: number;
  after: number;
  delta: number;
  mtime: number;
}

export interface Diff {
  older: Snapshot;
  newer: Snapshot;
  onDiskDelta: number;
  filesDelta: number;
  freeDelta: number | null;
  grew: DirChange[];
  shrank: DirChange[];
  filesUp: FileChange[];
  filesDown: FileChange[];
}

export interface HistoryInfo {
  snapshots: number;
  path: string;
}

export type FindingKind = "low-space" | "filling-fast" | "old-large" | "reclaimable" | "no-scan" | "no-trend" | "ai-leftovers";

export interface Finding {
  kind: FindingKind;
  severity: "info" | "warn" | "bad";
  penalty: number;
  subject: string;
  value: number;
  bytes: number;
  fix: "oldlarge" | "analyze" | "suggestions" | "home" | "devcache";
}

export interface HealthScore {
  score: number;
  grade: "good" | "fair" | "poor";
  findings: Finding[];
}

export interface AgeRow {
  id: number;
  name: string;
  kind: EntryKind;
  /** The folder's own files. */
  direct: boolean;
  total: number;
  bytes: number[];
  files: number[];
}

/** Bytes by last-modified age (A9). Columns: < 30 d, < 6 m, < 1 y, < 2 y, < 5 y, older, unknown. */
export interface AgeMatrix {
  bucketDays: number[];
  rows: AgeRow[];
  other: number[];
  totals: number[];
  files: number[];
}

export type Risk = "safe" | "caution" | "advanced" | "protected";

/** Folder Encyclopedia article (A5), already in the UI language. */
export interface Article {
  id: string;
  risk: Risk;
  title: string;
  /** Markdown subset, see `md.ts`. */
  body: string;
  /** The entry is inside the folder the article is about. */
  inside: boolean;
  path: string;
  rules: string[];
}

export type BlockReason =
  | "volume-root"
  | "system-dir"
  | "protected"
  | "contains-protected"
  | "link"
  | "contains-mount-point"
  | "unresolvable"
  | "stream"
  | "device"
  | "cross-volume"
  | "system-file"
  | "system-binary"
  | "cloud-file"
  | "changed"
  | "protected-level"
  | "nested";

export interface DeleteItem {
  id: number;
  name: string;
  path: string;
  kind: EntryKind;
  onDisk: number;
  files: number;
  allowed: boolean;
  reason: BlockReason | null;
  template: string | null;
}

export interface DeletePlan {
  items: DeleteItem[];
  onDisk: number;
  files: number;
  extraConfirm: boolean;
}

export interface RecycleOutcome {
  id: number;
  status: "done" | "declined" | "blocked" | "failed";
  reason: BlockReason | null;
  message: string | null;
  onDisk: number;
}

export interface CmdError {
  code: string;
  message: string;
}

/** Attribute bits, mirroring `nfd_core::model::Attrs`. */
export const ATTR = {
  system: 1 << 0,
  hidden: 1 << 1,
  reparse: 1 << 2,
  compressed: 1 << 3,
  sparse: 1 << 4,
  offline: 1 << 5,
  readonly: 1 << 6,
} as const;

/** Settings → Updates (`update_check`). */
export interface UpdateInfo {
  current: string;
  /** Newer version, if any. */
  available: string | null;
  notes: string | null;
  date: string | null;
  /** installer = NSIS/MSI (updates in place); setup / portable = release page. */
  kind: "installer" | "setup" | "portable";
  releases: string;
}

// ── Advice (Analyze → Suggestions, nfd-advice) ─────────────────────────────────

/** Can it go? `safe` → one click, `caution` → explicit tick, `guide` → only the proper
 * tool, `keep` → never, `unknown` → nothing known (the user decides). */
export type Verdict = "safe" | "caution" | "guide" | "keep" | "unknown";

export type AdviceReason =
  | "rule"
  | "keep-rule"
  | "build-output"
  | "agent-worktree"
  | "agent-leftover"
  | "agent-history"
  | "system"
  | "programs"
  | "protected"
  | "personal"
  | "explained"
  | "old-large"
  | "old-installer"
  | "duplicate";

export type AdviceAction = "clean" | "quarantine" | "ai-leftovers" | "recycle-bin" | "store" | "duplicates" | "guide" | "none";

export type RiskLevel = "safe" | "caution" | "advanced" | "protected";

/** Bytes on disk by verdict. */
export interface ByVerdict {
  safe: number;
  caution: number;
  guide: number;
  keep: number;
  unknown: number;
}

export interface BiText {
  en: string;
  vi: string;
}

export interface NodeAdvice {
  /** The verdict of the unit the entry belongs to (none: mixed or unknown). */
  verdict: Verdict | null;
  reason: AdviceReason | null;
  /** The entry starts its unit. */
  root: boolean;
  inside: ByVerdict;
  /** 🟢 + 🟡 bytes an unblocked action can free. */
  reclaim: number;
}

export interface AdviceUnit {
  entry: number;
  verdict: Verdict;
  reason: AdviceReason;
  level: RiskLevel | null;
  source: string | null;
  app: string | null;
  title: BiText | null;
  impact: BiText;
  untouched: BiText | null;
  action: AdviceAction;
  blocked: string | null;
  selected: boolean;
  store: string | null;
  requiresClosed: string[];
  cutoff: number | null;
}

export interface AdvicePart {
  id: number;
  path: string;
  verdict: Verdict;
  reason: AdviceReason;
  source: string | null;
  app: string | null;
  title: BiText | null;
  blocked: string | null;
  bytes: number;
}

export interface EntryAdvice {
  id: number;
  node: NodeAdvice;
  unit: AdviceUnit | null;
  /** Its suggestion group key. */
  group: string | null;
  /** Where the unit starts, when not at this entry (relative to the scan). */
  unitPath: string | null;
  parts: AdvicePart[];
  enriched: boolean;
}

export interface AdviceGroupItem {
  id: number | null;
  path: string;
  bytes: number;
  files: number;
  blocked: string | null;
}

export interface AdviceGroup {
  key: string;
  verdict: Verdict;
  reason: AdviceReason;
  level: RiskLevel | null;
  source: string | null;
  app: string | null;
  title: BiText | null;
  impact: BiText;
  untouched: BiText | null;
  action: AdviceAction;
  blocked: string | null;
  store: string | null;
  requiresClosed: string[];
  bytes: number;
  recent: number;
  files: number;
  count: number;
  selected: boolean;
  actionable: boolean;
  location: string | null;
  elsewhere: boolean;
  items: AdviceGroupItem[];
}

export interface Suggestions {
  totals: ByVerdict;
  reclaim: number;
  groups: AdviceGroup[];
  enriched: boolean;
}

export interface AdviceSummary {
  computing: boolean;
  seq: number;
  root: string;
  suggestions: Suggestions | null;
  elsewhere: AdviceGroup[];
}

export interface AdviceOutcome {
  operation: { freed: number; done: number; partial: number; skipped: number; failed: number; cancelled: boolean } | null;
  recycled: RecycleOutcome[];
  skipped: [number, string][];
}
