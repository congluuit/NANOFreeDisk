// Shapes of the per-website browser cache commands (src-tauri/src/commands/browsers.rs,
// crates/nfd-browsers).

export type StoreKind = "http-cache" | "code-cache-js" | "code-cache-wasm" | "cache-storage" | "firefox-cache";
export type StoreFormat = "blockfile" | "simple" | "cache-storage" | "cache2" | "unknown";

export interface StoreReport {
  kind: StoreKind;
  path: string;
  format: StoreFormat;
  /** Entries can be removed one site at a time (not the blockfile HTTP cache). */
  cleanable: boolean;
  entries: number;
  bytes: number;
  otherBytes: number;
  damaged: number;
  error: string | null;
}

export interface SiteRow {
  site: string;
  bytes: number;
  entries: number;
  cleanableBytes: number;
  cleanableEntries: number;
  /** In the blockfile HTTP cache: listed, cleaned only with the whole cache. */
  listedBytes: number;
  stores: StoreKind[];
}

export interface ProfileReport {
  name: string;
  path: string;
  stores: StoreReport[];
  sites: SiteRow[];
}

export interface BrowserReport {
  id: string;
  name: string;
  ruleId: string;
  exe: string;
  running: boolean;
  profiles: ProfileReport[];
}

export interface SitesReport {
  browsers: BrowserReport[];
  truncated: boolean;
}

export interface SiteTarget {
  browser: string;
  profile: string | null;
  site: string;
  subdomains: boolean;
}

export interface StoreTotal {
  kind: StoreKind;
  entries: number;
  bytes: number;
}

export interface SitePlan {
  items: number;
  entries: number;
  bytes: number;
  byStore: StoreTotal[];
  indexFiles: number;
  listedOnly: number;
  running: string[];
  refused: number;
}
