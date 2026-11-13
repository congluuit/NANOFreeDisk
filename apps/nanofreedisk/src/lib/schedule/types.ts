// Shapes of the automatic cleaning commands (src-tauri/src/commands/schedule.rs,
// docs/SCHEDULE.md).

export type Frequency = "off" | "weekly" | "monthly";

/** The user's choices (`crate::schedule::Config`). */
export interface ScheduleConfig {
  frequency: Frequency;
  /** Wait until the PC has had no input for a few minutes. */
  idleOnly: boolean;
  /** Rule categories left out of automatic cleaning (can only narrow). */
  offCategories: string[];
  /** Low disk space alert on. */
  lowSpace: boolean;
  /** Alert below this share of the drive… */
  lowPercent: number;
  /** …or below this many GB, whichever is smaller. */
  lowGb: number;
}

export interface LastRun {
  at: number;
  freed: number;
  items: number;
  /** Journal id (Cleanup history). */
  op: string | null;
}

export interface Autostart {
  supported: boolean;
  on: boolean;
  /** Turned off in Task Manager's Startup apps. */
  blocked: boolean;
}

export interface ScheduleState {
  config: ScheduleConfig;
  /** Automatic cleaning exists on this platform (Windows). */
  supported: boolean;
  last: LastRun | null;
  /** Earliest time of the next automatic clean (Unix seconds). */
  next: number | null;
  /** Categories the picker offers. */
  categories: string[];
  autostart: Autostart;
  /** The app was started in the tray. */
  trayStart: boolean;
}

export interface PreviewRule {
  ruleId: string;
  app: string;
  category: string;
  bytes: number;
  items: number;
}

export interface SchedulePreview {
  rules: PreviewRule[];
  bytes: number;
  items: number;
}
