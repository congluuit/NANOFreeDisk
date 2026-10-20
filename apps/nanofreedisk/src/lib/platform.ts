// What each platform has (docs/MACOS.md §4). The backend reports its OS in
// `app_info`; until that answer arrives the user agent decides, so a Mac never
// shows Windows-only views for a moment. The mock ("browser") shows everything.
//
// Hidden on macOS: Clean (the rule packs are Windows'), Drive health (SMART/NVMe
// through the elevated helper), Hardware (sensors, Tasks, PawnIO) and the disk
// activity Monitor (ETW), plus their Settings blocks, the Explorer menu and the
// in-place updater.

import type { View } from "./state.svelte";

export type Feature =
  | "clean"
  | "health"
  | "hardware"
  | "monitor"
  | "sensors"
  | "shell-menu"
  | "updater";

const WINDOWS_ONLY: readonly Feature[] = [
  "clean",
  "health",
  "hardware",
  "monitor",
  "sensors",
  "shell-menu",
  "updater",
];

const VIEW_FEATURE: Partial<Record<View, Feature>> = {
  clean: "clean",
  health: "health",
  hardware: "hardware",
  monitor: "monitor",
};

/** The OS: `app_info`'s answer, else a guess from the user agent. */
export function currentOs(reported: string | null | undefined): string {
  if (reported) return reported;
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
  return /Mac OS X|Macintosh/.test(ua) ? "macos" : "windows";
}

export function hasFeature(os: string, feature: Feature): boolean {
  return os !== "macos" || !WINDOWS_ONLY.includes(feature);
}

export function viewAvailable(os: string, view: View): boolean {
  const feature = VIEW_FEATURE[view];
  return !feature || hasFeature(os, feature);
}

/** The key shortcuts are spelled with ⌘ on a Mac. */
export function modKey(os: string): string {
  return os === "macos" ? "⌘" : "Ctrl";
}
