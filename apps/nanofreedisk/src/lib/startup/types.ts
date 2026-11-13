// Shapes of the Startup commands (src-tauri/src/commands/startup.rs,
// crates/nfd-apps/src/startup.rs).

/** Where an entry is registered. */
export type StartupOrigin = "user-run" | "machine-run" | "machine-run32" | "user-folder" | "common-folder";

/** One program that starts with Windows. */
export interface StartupEntry {
  /** `<origin>|<value or file name>` */
  id: string;
  name: string;
  /** The registered command (or the shortcut's target and arguments). */
  command: string;
  /** The program that runs (for rundll32, the DLL it loads). */
  exe: string | null;
  /** The program is not on the disk. */
  missing: boolean;
  /** Signer, or `CompanyName *` when the file is not signed. */
  publisher: string | null;
  origin: StartupOrigin;
  scope: "user" | "machine";
  /** The registry key or folder it is registered in. */
  location: string;
  enabled: boolean;
  /** When it was turned off (Unix seconds), as Task Manager recorded it. */
  disabledAt: number | null;
  caution: "windows" | "security" | null;
  approvedName: string;
}

export interface StartupIcon {
  src: string;
  plate: boolean;
}
