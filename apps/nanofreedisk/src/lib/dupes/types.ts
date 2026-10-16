// Shapes of the Duplicates commands (src-tauri/src/commands/dupes.rs,
// crates/nfd-dupes).

export interface CopyFlags {
  /** Inside a OneDrive / Dropbox / Google Drive folder. */
  cloud: boolean;
  /** Inside a git working tree. */
  git: boolean;
  /** Inside an application folder (only listed with "include app folders"). */
  appDir: boolean;
  readonly: boolean;
}

export interface FileCopy {
  path: string;
  mtime: number;
  /** Hex text. */
  volume: string;
  fileId: string | null;
  /** Other names of the same file (hard links). */
  links?: string[];
  flags: CopyFlags;
}

export interface DupeGroup {
  id: number;
  hash: string;
  size: number;
  wasted: number;
  copies: FileCopy[];
}

export type SkipReason =
  | "empty"
  | "small"
  | "extension"
  | "cloud"
  | "reparse"
  | "system"
  | "protected"
  | "app-dir"
  | "unreadable"
  | "changed";

export interface DupeReport {
  roots: string[];
  minSize: number;
  groups: DupeGroup[];
  wastedBytes: number;
  scannedFiles: number;
  candidates: number;
  hashedBytes: number;
  cacheHits: number;
  skipped: Partial<Record<SkipReason, number>>;
  skippedSamples: { path: string; reason: SkipReason }[];
  elapsedMs: number;
  paranoid: boolean;
  /** Groups found; `groups` holds at most 5,000. */
  totalGroups: number;
}

export type DupePhase = "scan" | "collect" | "partial" | "full" | "compare";

export interface DupeProgress {
  phase: DupePhase;
  done: number;
  total: number;
  bytes: number;
  totalBytes: number;
  path: string;
}

export interface FindOptions {
  minSize: number;
  includeAppDirs: boolean;
  paranoid: boolean;
  extensions: string[];
  useCache: boolean;
}

/** What to do with one group (`nfd_dupes::keep::Selection`). */
export interface Selection {
  group: number;
  keep: string;
  remove: string[];
}

export type KeepStrategy = "auto" | "oldest" | "newest" | "shortest";
export type DupeAction = "recycle" | "quarantine";
/** Which folders to search: the Analyze scan, or folders picked here. */
export type Scope = "current" | "folders";
export type FileKind = "all" | "images" | "videos" | "music" | "documents" | "archives";
