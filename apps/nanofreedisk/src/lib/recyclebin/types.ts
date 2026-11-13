// Shapes of the Recycle Bin commands (src-tauri/src/commands/recyclebin.rs).

/** What the bin of one drive holds. */
export interface BinDrive {
  /** `C:\` */
  root: string;
  bytes: number;
  items: number;
}

/** What the Recycle Bin holds. */
export interface BinStatus {
  bytes: number;
  items: number;
  /** Drives whose bin holds something. */
  drives: BinDrive[];
  /** Activity log items found gone from the bin just now (reload the Activity log when not 0). */
  settled: number;
}

/** How emptying went. */
export interface EmptyOutcome {
  bytes: number;
  items: number;
  /** `C:\: message` for drives whose bin could not be emptied. */
  failed: string[];
  settled: number;
  journalError: string | null;
}
