// "Not compared": the files a search left out, by reason, each with what can be done
// about it (a search with a lower size limit, with application folders shown, with
// every file type, or simply again).

import { lowerMinSize } from "./size.ts";
import type { DupeReport, SkipReason } from "./types.ts";

/** What a reason offers. */
export type SkipAction =
  | { kind: "lower"; bytes: number }
  | { kind: "app-dirs" }
  | { kind: "all-types" }
  | { kind: "again" };

export interface SkipLine {
  reason: SkipReason;
  count: number;
  action: SkipAction | null;
  /** A few files it concerns (never for small or empty files). */
  samples: string[];
}

/** Shown in this order: what the user can most likely act on first. */
const ORDER: SkipReason[] = ["small", "app-dir", "extension", "cloud", "protected", "changed", "unreadable", "system", "reparse", "clone", "empty"];

const SAMPLES = 3;

/** One line per reason with files, in display order. */
export function skipLines(report: Pick<DupeReport, "skipped" | "skippedSamples" | "minSize">): SkipLine[] {
  const out: SkipLine[] = [];
  for (const reason of ORDER) {
    const count = report.skipped[reason] ?? 0;
    if (!count) continue;
    let action: SkipAction | null = null;
    if (reason === "small") {
      const bytes = lowerMinSize(report.minSize);
      if (bytes !== null) action = { kind: "lower", bytes };
    } else if (reason === "app-dir") action = { kind: "app-dirs" };
    else if (reason === "extension") action = { kind: "all-types" };
    else if (reason === "changed") action = { kind: "again" };
    const samples = report.skippedSamples
      .filter((s) => s.reason === reason)
      .slice(0, SAMPLES)
      .map((s) => s.path);
    out.push({ reason, count, action, samples });
  }
  return out;
}
