// Shapes of the "Similar photos" commands (src-tauri/src/commands/similar.rs,
// crates/nfd-dupes/src/similar.rs).

import type { CopyFlags } from "../dupes/types";

/** How alike two pictures must be (`nfd_dupes::similar::Preset`). */
export type Preset = "strict" | "normal" | "loose";

/** A picture in a group. */
export interface Photo {
  path: string;
  /** Other names of the same file (hard links): removed with it. */
  links?: string[];
  size: number;
  mtime: number;
  /** Upright size in pixels. */
  width: number;
  height: number;
  /** Hex text. */
  volume: string;
  fileId: string | null;
  flags: CopyFlags;
  /** Bits (of 512) that differ from the group's first photo. */
  distance: number;
  /** Byte for byte the same file as this earlier photo of the group. */
  identicalTo?: string;
}

/** Pictures that look alike; the first one is the suggested best. */
export interface SimilarGroup {
  id: number;
  photos: Photo[];
  /** Bytes of every photo but the first, identical copies left out. */
  extra: number;
}

export type SimilarSkip =
  | "small"
  | "cloud"
  | "reparse"
  | "system"
  | "protected"
  | "app-dir"
  | "unreadable"
  | "changed"
  | "too-large"
  | "plain";

export interface SimilarReport {
  roots: string[];
  minSize: number;
  preset: Preset;
  groups: SimilarGroup[];
  extraBytes: number;
  scannedFiles: number;
  /** Pictures compared. */
  images: number;
  /** Pictures decoded in this run (the rest came from the cache). */
  decoded: number;
  cacheHits: number;
  skipped: Partial<Record<SimilarSkip, number>>;
  skippedSamples: { path: string; reason: SimilarSkip }[];
  elapsedMs: number;
  /** Groups found; `groups` holds at most 2,000. */
  totalGroups: number;
}

export type SimilarPhase = "scan" | "collect" | "hash" | "compare";

export interface SimilarProgress {
  phase: SimilarPhase;
  done: number;
  total: number;
  path: string;
}

export interface SimilarFindOptions {
  /** Smallest picture compared, bytes (default 10 KiB). */
  minSize?: number;
  preset: Preset;
  includeAppDirs: boolean;
  useCache: boolean;
  /** Search the personal folders (Documents, Downloads, Pictures…) instead of `paths`. */
  personal: boolean;
}

/** The photos of one group to remove (`nfd_dupes::similar::plan::SimilarSelection`). */
export interface SimilarSelection {
  group: number;
  remove: string[];
  /** The user confirmed that no photo of this group stays. */
  keepNone?: boolean;
}

export type SimilarAction = "recycle" | "quarantine";
/** Where to look: the personal folders, the Analyze scan, or folders picked here. */
export type SimilarScope = "personal" | "current" | "folders";
/** Which half of the Duplicates view is shown. */
export type DupesMode = "identical" | "similar";
