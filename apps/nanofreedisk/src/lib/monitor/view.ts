// Pure helpers of the Monitor view (tested in monitor.test.ts).

import type { DiskStat, FileRate, Minute, Point, ProcRate, TopProc } from "./types";

export type SortKey = "name" | "pid" | "read" | "write" | "iops" | "files";

export const iops = (p: ProcRate) => p.readOps + p.writeOps;

export function sortProcs(list: ProcRate[], key: SortKey, desc: boolean): ProcRate[] {
  const val = (p: ProcRate): number | string => {
    switch (key) {
      case "name":
        return p.name.toLowerCase();
      case "pid":
        return p.pid;
      case "read":
        return p.read;
      case "write":
        return p.write;
      case "iops":
        return iops(p);
      case "files":
        return p.files;
    }
  };
  const out = [...list];
  out.sort((a, b) => {
    const x = val(a);
    const y = val(b);
    const c = typeof x === "string" ? x.localeCompare(y as string) : x - (y as number);
    return (desc ? -c : c) || a.pid - b.pid;
  });
  return out;
}

/** Files of one process, most written first. */
export function filesOf(files: FileRate[], pid: number): FileRate[] {
  return files.filter((f) => f.pid === pid).sort((a, b) => b.write - a.write || b.read - a.read);
}

/** Folder of a file path (Windows separators). */
export function parentDir(path: string): string {
  const i = path.lastIndexOf("\\");
  if (i <= 0) return path;
  // keep "C:\" for files at the root of a drive
  return i === 2 && path[1] === ":" ? path.slice(0, 3) : path.slice(0, i);
}

/** A round axis maximum ≥ v (1, 2, 5 × 1024ⁿ steps), at least 1 MB/s. */
export function niceMax(v: number): number {
  let unit = 1024 * 1024;
  while (unit * 1024 <= v) unit *= 1024;
  for (const m of [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000]) if (unit * m >= v) return unit * m;
  return unit * 1024;
}

/**
 * SVG path ("M x y L …") of one series over the last `seconds` seconds ending at
 * `end` (Unix seconds), in a `w` × `h` box with `max` at the top. Gaps over 3 s
 * (a pause) start a new segment.
 */
export function seriesPath(points: Point[], field: "read" | "write", end: number, seconds: number, w: number, h: number, max: number): string {
  if (!points.length || max <= 0) return "";
  const start = end - seconds;
  let d = "";
  let last = -Infinity;
  for (const p of points) {
    if (p.t < start) continue;
    const x = ((p.t - start) / seconds) * w;
    const y = h - (Math.min(p[field], max) / max) * h;
    d += `${p.t - last > 3 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)} `;
    last = p.t;
  }
  return d.trim();
}

/** "0 C: E:" → { index: "0", letters: "C: E:" }. */
export function diskLabel(d: DiskStat): { index: string; letters: string } {
  const m = /^(\d+)\s*(.*)$/.exec(d.name.trim());
  return m ? { index: m[1], letters: m[2] } : { index: "", letters: d.name };
}

export interface HourBar {
  /** Unix seconds of the hour's start. */
  ts: number;
  read: number;
  write: number;
  minutes: number;
}

/** The last 24 hours by hour (oldest first), from the saved minutes. */
export function hourBars(minutes: Minute[], nowSecs: number): HourBar[] {
  const endHour = Math.floor(nowSecs / 3600) * 3600;
  const bars: HourBar[] = [];
  for (let i = 23; i >= 0; i--) bars.push({ ts: endHour - i * 3600, read: 0, write: 0, minutes: 0 });
  for (const m of minutes) {
    const idx = 23 - (endHour - Math.floor(m.ts / 3600) * 3600) / 3600;
    if (idx < 0 || idx > 23) continue;
    bars[idx].read += m.read;
    bars[idx].write += m.write;
    bars[idx].minutes += 1;
  }
  return bars;
}

export const totalOf = (list: TopProc[], field: "read" | "write") => list.reduce((a, t) => a + t[field], 0);
