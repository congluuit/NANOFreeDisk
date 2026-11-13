// What a clean left behind and what the Clean view offers for it
// (`nfd_core::exec::group_leftovers`): a title, a plain-language explanation and one
// action per cause. Pure, so the test can check every key against the locales.

import type { LeftoverGroup, OperationResult, Tool } from "./types";

export type LeftoverAction = "retry" | "admin" | "retry-other";

/** The groups of a result, in the backend's order (in use, administrator, other). */
export function leftoverGroups(r: OperationResult | null | undefined): LeftoverGroup[] {
  return (r?.leftovers ?? []).filter((g) => g.items.length > 0);
}

/** Locale key and arguments of a group's title. */
export function leftoverTitle(g: LeftoverGroup): { key: string; args: Record<string, string> } {
  switch (g.cause) {
    case "in-use":
      return g.programs.length
        ? { key: "clean-left-in-use-title", args: { apps: g.programs.join(", ") } }
        : { key: "clean-left-in-use-unknown", args: {} };
    case "needs-admin":
      return { key: "clean-left-admin-title", args: {} };
    default:
      return { key: "clean-left-other-title", args: {} };
  }
}

/** Locale key of a group's explanation. */
export function leftoverBody(g: LeftoverGroup): string {
  return g.cause === "in-use" ? "clean-left-in-use-body" : g.cause === "needs-admin" ? "clean-left-admin-body" : "clean-left-other-body";
}

/** What the user can do about a group: close the program and retry, clean with
 *  administrator rights, or look again (only when something changed since the scan). */
export function leftoverAction(g: LeftoverGroup): LeftoverAction | null {
  if (g.cause === "in-use") return "retry";
  if (g.cause === "needs-admin") return "admin";
  return g.items.some((i) => i.reason === "changed") ? "retry-other" : null;
}

/** Locale key of an action's button. */
export const ACTION_KEYS: Record<LeftoverAction, string> = {
  retry: "clean-left-retry",
  admin: "clean-left-admin-button",
  "retry-other": "clean-left-other-retry",
};

/** Locale key of the button that opens a rule's Windows tool. */
export const toolKey = (tool: Tool): string => `clean-tool-${tool}`;

export const TOOLS: Tool[] = ["disk-cleanup", "storage-settings", "system-protection", "performance-options"];

/** Every key the leftovers UI uses (checked in both languages by the tests). */
export const LEFTOVER_KEYS: string[] = [
  "clean-left-title",
  "clean-left-in-use-title",
  "clean-left-in-use-unknown",
  "clean-left-in-use-body",
  "clean-left-admin-title",
  "clean-left-admin-body",
  "clean-left-other-title",
  "clean-left-other-body",
  "clean-left-size",
  "clean-left-more",
  ...Object.values(ACTION_KEYS),
  "clean-admin-title",
  "clean-admin-body",
  "clean-admin-where",
  "clean-admin-where-permanent",
  "clean-admin-go",
  "clean-admin-declined",
  "clean-admin-disabled",
  "clean-retry-nothing",
  "clean-rule-admin",
  "clean-rule-admin-hint",
  "clean-tool-hint",
  ...TOOLS.map(toolKey),
  "activity-kind-clean-admin",
];

/** Reason codes only the elevated run and the retry send (`nfd_rules::elevated`). */
export const ELEVATED_REASONS = [
  "relative-path",
  "unknown-rule",
  "not-admin-rule",
  "expert-only",
  "not-matched",
  "kept",
  "not-found-now",
  "path-changed",
];
