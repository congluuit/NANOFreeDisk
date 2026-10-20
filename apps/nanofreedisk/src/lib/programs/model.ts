// Pure helpers of the Programs view (tested in programs.test.ts): filtering and
// sorting the program list, and the selection items of the leftover and software
// junk lists.

import type { SelItem } from "../selection";
import type { JunkReport, Leftover, LeftoverKind, LeftoverReport, Program } from "./types";

export type ProgramFilter = "all" | "win32" | "store" | "user" | "large" | "broken";
export type ProgramSort = "name" | "size" | "date";

/** Programs at least this big count as "large". */
export const LARGE_BYTES = 1024 ** 3;

/** Lower-case text a search matches: name, publisher, version, install folder. */
function haystack(p: Program): string {
  return [p.name, p.publisher, p.version, p.installLocation].filter(Boolean).join(" ").toLowerCase();
}

/** The programs to show for `query` (every word must match), `filter` and `sort`. */
export function visiblePrograms(list: Program[], query: string, filter: ProgramFilter, sort: ProgramSort): Program[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const out = list.filter((p) => {
    if (words.length && !words.every((w) => haystack(p).includes(w))) return false;
    switch (filter) {
      case "win32":
        return p.source !== "store";
      case "store":
        return p.source === "store";
      case "user":
        return p.scope === "user";
      case "large":
        return p.size >= LARGE_BYTES;
      case "broken":
        return p.broken;
      default:
        return true;
    }
  });
  const byName = (a: Program, b: Program) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  if (sort === "size") out.sort((a, b) => b.size - a.size || byName(a, b));
  else if (sort === "date") out.sort((a, b) => (b.installedAt ?? 0) - (a.installedAt ?? 0) || byName(a, b));
  else out.sort(byName);
  return out;
}

/** Can the user start removing `p` here (its uninstaller, or its leftovers)? */
export function removable(p: Program): boolean {
  return !p.blocked;
}

/** The letter shown in place of an icon. */
export function initial(name: string): string {
  const m = /[\p{L}\p{N}]/u.exec(name);
  return (m?.[0] ?? "?").toUpperCase();
}

/** A stable hue (0–359) for a program's initial. */
export function hue(name: string): number {
  let h = 0;
  for (const c of name) h = (h * 31 + c.codePointAt(0)!) % 360;
  return h;
}

/** Display groups of the leftover list, in order. */
export const LEFTOVER_GROUPS: { id: string; kinds: LeftoverKind[] }[] = [
  { id: "files", kinds: ["folder", "file"] },
  { id: "shortcuts", kinds: ["shortcut"] },
  { id: "registry", kinds: ["registry-key", "registry-value"] },
  { id: "services", kinds: ["service"] },
];

export function groupOf(l: Leftover): string {
  return LEFTOVER_GROUPS.find((g) => g.kinds.includes(l.kind))?.id ?? "files";
}

/** Selection items of a leftover report (services are information only). */
export function leftoverItems(r: LeftoverReport): SelItem[] {
  return r.items.map((i) => ({
    id: i.id,
    bytes: i.bytes,
    files: i.files,
    level: i.level,
    group: groupOf(i),
    locked: !!i.blocked || i.kind === "service",
  }));
}

/** Selection items of the software junk (group = program key). */
export function junkItems(r: JunkReport | null): SelItem[] {
  return (r?.groups ?? []).flatMap((g) =>
    g.items.map((i) => ({
      id: i.path,
      bytes: i.bytes,
      files: i.files,
      level: i.level,
      group: g.key,
      locked: !!i.blocked,
      skip: g.running ? g.requiresClosed[0] ?? null : null,
    })),
  );
}

/** The registry path as people read it (`HKCU\Software\…`). */
export function shortHive(path: string): string {
  return path.replace(/^HKEY_CURRENT_USER/i, "HKCU").replace(/^HKEY_LOCAL_MACHINE/i, "HKLM");
}

/** The `nfd` command for the CLI hint. */
export function uninstallCommand(p: Program): string {
  const name = p.name.replace(/"/g, '\\"');
  return p.broken ? `nfd programs leftovers "${name}" --apply` : `nfd programs uninstall "${name}" --remove-leftovers`;
}
