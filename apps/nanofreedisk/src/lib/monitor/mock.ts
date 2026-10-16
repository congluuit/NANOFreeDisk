// Browser-only stand-in for the Monitor commands (`pnpm dev` without Tauri): a
// few processes with plausible disk activity, file-level detail when "enabled".

import type { FileRate, MonitorHistory, MonitorView, Point, ProcRate, Snapshot, SpamConfig, StampedAlert, TouchRow } from "./types";

const MB = 1024 * 1024;

const PROCS: [number, string, number, number][] = [
  // pid, name, read MB/s, write MB/s
  [4120, "Code.exe", 0.4, 0.2],
  [7788, "chrome.exe", 1.2, 0.9],
  [3010, "MsMpEng.exe", 6.5, 0.1],
  [9120, "OneDrive.exe", 0.3, 2.4],
  [6644, "noisy-logger.exe", 0, 8.2],
  [5150, "SearchIndexer.exe", 2.1, 0.6],
];

const FILES: Record<number, string[]> = {
  4120: ["C:\\Users\\nano\\AppData\\Roaming\\Code\\logs\\main.log", "C:\\Users\\nano\\Projects\\app\\src\\main.ts"],
  7788: ["C:\\Users\\nano\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\Cache_Data\\data_1"],
  3010: ["C:\\ProgramData\\Microsoft\\Windows Defender\\Scans\\mpcache-1.bin", "C:\\Users\\nano\\Downloads\\setup.exe"],
  9120: ["C:\\Users\\nano\\OneDrive\\Photos\\IMG_2024.jpg"],
  6644: ["C:\\Tools\\noisy\\logs\\debug.log"],
  5150: ["C:\\ProgramData\\Microsoft\\Search\\Data\\Applications\\Windows\\Windows.edb"],
};

const state = {
  paused: false,
  detail: false,
  watch: null as string | null,
  config: { enabled: false, mbps: 5, minutes: 5 } as SpamConfig,
  points: [] as Point[],
  touches: [] as TouchRow[],
  alerts: [] as StampedAlert[],
  last: null as Snapshot | null,
  tick: 0,
};

function wave(i: number, base: number): number {
  return Math.max(0, base * (0.6 + 0.4 * Math.sin(state.tick / 4 + i)));
}

function sample(): Snapshot {
  state.tick++;
  const now = Date.now();
  const processes: ProcRate[] = PROCS.map(([pid, name, r, w], i) => ({
    pid,
    createTime: String(133_900_000_000_000_000 + pid),
    name,
    path: `C:\\Program Files\\${name.replace(".exe", "")}\\${name}`,
    read: wave(i, r * MB),
    write: wave(i + 1, w * MB),
    readOps: wave(i, r * 40),
    writeOps: wave(i + 1, w * 25),
    files: state.detail ? FILES[pid].length : 0,
  }));
  processes.sort((a, b) => b.read + b.write - (a.read + a.write));
  const files: FileRate[] = state.detail
    ? processes.flatMap((p) =>
        FILES[p.pid].map((path, j) => ({
          pid: p.pid,
          process: p.name,
          path,
          read: p.read / (j + 1),
          write: p.write / (j + 1),
          ops: (p.readOps + p.writeOps) / (j + 1),
        })),
      )
    : [];
  files.sort((a, b) => b.write - a.write);
  const read = processes.reduce((a, p) => a + p.read, 0);
  const write = processes.reduce((a, p) => a + p.write, 0);
  state.points.push({ t: Math.floor(now / 1000), read, write });
  if (state.points.length > 600) state.points.shift();
  const touches: TouchRow[] = [];
  if (state.detail && state.watch && state.tick % 3 === 0) {
    const ops = ["create", "write", "rename", "delete"] as const;
    const op = ops[state.tick % 4];
    touches.push({
      at: now,
      pid: 4120,
      process: "Code.exe",
      op,
      path: `${state.watch}\\notes-${state.tick % 5}.md`,
      target: op === "rename" ? `${state.watch}\\notes-${state.tick % 5}.old` : null,
    });
    state.touches.unshift(...touches);
    state.touches.length = Math.min(state.touches.length, 500);
  }
  return {
    schema: "nanofreedisk.monitor.v1",
    at: now,
    source: state.detail ? "helper" : "counters",
    intervalMs: 1000,
    read,
    write,
    ops: processes.reduce((a, p) => a + p.readOps + p.writeOps, 0),
    processes,
    files,
    disks: [
      { name: "0 C:", read: read * 0.6, write: write * 0.5, transfers: 180, queue: 0.4, latencyMs: 0.8, idle: 82 },
      { name: "1 D:", read: read * 0.1, write: write * 0.2, transfers: 12, queue: 0.02, latencyMs: 4.1, idle: 97 },
    ],
    touches,
    lost: 0,
    undecoded: 0,
    alerts: [],
  };
}

function view(): MonitorView {
  if (!state.paused) state.last = sample();
  return {
    snapshot: state.last,
    paused: state.paused,
    elevated: false,
    detail: { wanted: state.detail, running: state.detail && !state.paused, remote: true, error: null },
    watch: state.watch,
    alerts: state.alerts,
    touches: state.touches,
    config: state.config,
  };
}

function history(): MonitorHistory {
  const now = Math.floor(Date.now() / 60_000) * 60;
  const minutes = [];
  for (let i = 0; i < 24 * 60; i += 3) {
    const ts = now - i * 60;
    const busy = Math.abs(Math.sin(i / 90));
    minutes.push({
      ts,
      read: Math.round(busy * 400 * MB),
      write: Math.round(busy * 250 * MB),
      peakRead: busy * 30 * MB,
      peakWrite: busy * 20 * MB,
      seconds: 60,
      top: [
        { name: "noisy-logger.exe", read: 0, write: Math.round(busy * 120 * MB) },
        { name: "MsMpEng.exe", read: Math.round(busy * 300 * MB), write: Math.round(busy * 5 * MB) },
      ],
    });
  }
  minutes.reverse();
  return {
    minutes,
    top: [
      { name: "noisy-logger.exe", read: 0, write: 52 * 1024 * MB },
      { name: "OneDrive.exe", read: 2 * 1024 * MB, write: 9 * 1024 * MB },
      { name: "MsMpEng.exe", read: 80 * 1024 * MB, write: 2 * 1024 * MB },
    ],
  };
}

export const monitorHandlers: Record<string, (args: Record<string, unknown>) => unknown> = {
  monitor_snapshot: () => view(),
  monitor_chart: (a) => {
    const secs = Number(a.seconds) || 60;
    const end = state.points.at(-1)?.t ?? 0;
    return state.points.filter((p) => p.t > end - secs);
  },
  monitor_pause: (a) => (state.paused = Boolean(a.paused)),
  monitor_detail: (a) => {
    state.detail = Boolean(a.enabled);
    return view();
  },
  monitor_watch: (a) => {
    state.watch = (a.path as string | null) ?? null;
    state.touches = [];
    return view();
  },
  monitor_configure: (a) => {
    state.config = a.alerts as SpamConfig;
    if (state.config.enabled && !state.alerts.length) {
      state.alerts.push({
        at: Date.now(),
        kind: "log-spam",
        pid: 6644,
        createTime: String(133_900_000_000_000_000 + 6644),
        name: "noisy-logger.exe",
        path: "C:\\Tools\\noisy\\noisy-logger.exe",
        rate: 8.2 * MB,
        minutes: state.config.minutes,
        since: Date.now() - state.config.minutes * 60_000,
      });
    }
    return undefined;
  },
  monitor_history: () => history(),
  monitor_reveal: () => undefined,
  tray_texts: () => undefined,
};
