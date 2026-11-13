// Pure helpers of the Startup view (no Svelte, no backend): filtering, sorting
// and what an entry allows.

import type { StartupEntry } from "./types";

export type StartupFilter = "all" | "enabled" | "disabled" | "user" | "machine";
export type StartupSort = "name" | "status";

/** This user's entries change here; all users' entries need Task Manager. */
export const canChange = (e: StartupEntry): boolean => e.scope === "user";

/** Turning it off deserves a question first (Windows itself, security software). */
export const needsConfirm = (e: StartupEntry): boolean => e.caution !== null;

/** The publisher without the `*` that marks an unsigned file, and whether it was signed. */
export function publisherOf(e: StartupEntry): { name: string; verified: boolean } | null {
  const p = e.publisher?.trim();
  if (!p) return null;
  return p.endsWith(" *") ? { name: p.slice(0, -2).trim(), verified: false } : { name: p, verified: true };
}

function matches(e: StartupEntry, q: string): boolean {
  if (!q) return true;
  const hay = [e.name, e.command, e.publisher ?? "", e.exe ?? ""].join("\n").toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

const FILTERS: Record<StartupFilter, (e: StartupEntry) => boolean> = {
  all: () => true,
  enabled: (e) => e.enabled,
  disabled: (e) => !e.enabled,
  user: (e) => e.scope === "user",
  machine: (e) => e.scope === "machine",
};

/** The entries the list shows: matching `query` and `filter`, sorted by name or
 *  with the enabled ones first. */
export function visibleEntries(list: StartupEntry[], query: string, filter: StartupFilter, sort: StartupSort): StartupEntry[] {
  const byName = (a: StartupEntry, b: StartupEntry) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  return list
    .filter((e) => FILTERS[filter](e) && matches(e, query.trim()))
    .sort((a, b) => (sort === "status" && a.enabled !== b.enabled ? (a.enabled ? -1 : 1) : byName(a, b)));
}
