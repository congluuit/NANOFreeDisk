// Hardware → Tasks: filtering, sorting, grouping and the parent/child tree. Pure
// (tested in hw.test.ts).

import type { Task, TaskGroup } from "./types";

export type SortKey = "name" | "pid" | "cpu" | "memory" | "io" | "gpu" | "publisher";

export interface Sort {
  key: SortKey;
  desc: boolean;
}

export const GROUP_ORDER: TaskGroup[] = ["app", "background", "windows"];

const io = (t: Task) => t.io_read + t.io_write;

function cmp(a: Task, b: Task, key: SortKey): number {
  switch (key) {
    case "name":
      return a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) || a.pid - b.pid;
    case "pid":
      return a.pid - b.pid;
    case "cpu":
      return a.cpu - b.cpu;
    case "memory":
      return a.memory - b.memory;
    case "io":
      return io(a) - io(b);
    case "gpu":
      return a.gpu - b.gpu;
    case "publisher":
      return (a.publisher ?? "").localeCompare(b.publisher ?? "");
  }
}

export function sortTasks(list: Task[], sort: Sort): Task[] {
  const out = [...list].sort((a, b) => cmp(a, b, sort.key));
  return sort.desc ? out.reverse() : out;
}

/** Case-insensitive match on name, window title, PID, publisher or path. */
export function filterTasks(list: Task[], query: string): Task[] {
  const q = query.trim().toLowerCase();
  if (!q) return list;
  return list.filter(
    (t) =>
      t.name.toLowerCase().includes(q) ||
      String(t.pid) === q ||
      (t.title ?? "").toLowerCase().includes(q) ||
      (t.publisher ?? "").toLowerCase().includes(q) ||
      (t.path ?? "").toLowerCase().includes(q),
  );
}

export interface Row {
  task: Task;
  depth: number;
  /** Has children in the current list (tree mode). */
  parent: boolean;
}

/** Parent → children rows (children sorted the same way). A PID whose "parent" is
 * younger than itself is a reused PID and starts its own root. */
export function treeRows(list: Task[], sort: Sort, collapsed: Set<number> = new Set()): Row[] {
  const byPid = new Map(list.map((t) => [t.pid, t]));
  const kids = new Map<number, Task[]>();
  const roots: Task[] = [];
  for (const t of list) {
    const p = byPid.get(t.ppid);
    const older = p && BigInt(p.create_time) <= BigInt(t.create_time);
    if (p && p.pid !== t.pid && older) {
      const arr = kids.get(p.pid) ?? [];
      arr.push(t);
      kids.set(p.pid, arr);
    } else roots.push(t);
  }
  const out: Row[] = [];
  const walk = (t: Task, depth: number, seen: Set<number>) => {
    if (seen.has(t.pid)) return;
    seen.add(t.pid);
    const children = kids.get(t.pid) ?? [];
    out.push({ task: t, depth, parent: children.length > 0 });
    if (collapsed.has(t.pid)) return;
    for (const c of sortTasks(children, sort)) walk(c, depth + 1, seen);
  };
  const seen = new Set<number>();
  for (const r of sortTasks(roots, sort)) walk(r, 0, seen);
  return out;
}

export function groupTasks(list: Task[]): Record<TaskGroup, Task[]> {
  const out: Record<TaskGroup, Task[]> = { app: [], background: [], windows: [] };
  for (const t of list) out[t.group].push(t);
  return out;
}

/** "Disk" column: read + write, bytes / s. */
export const ioRate = io;
