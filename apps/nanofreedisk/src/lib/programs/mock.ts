// Browser-only stand-in for the Programs commands (`pnpm dev` without Tauri): a
// handful of programs, one with a missing uninstaller, leftovers after uninstalling
// and some software junk. Nothing touches a real disk or registry.

import type { OperationResult } from "../clean/types";
import type { JunkGroup, JunkReport, Leftover, LeftoverReport, Program } from "./types";

const MB = 2 ** 20;
const GB = 2 ** 30;
const DAY = 86_400;
const now = () => Math.floor(Date.now() / 1000);
const HOME = "C:\\Users\\nano";

function prog(id: string, name: string, publisher: string, size: number, age: number, extra: Partial<Program> = {}): Program {
  const machine = !id.startsWith("hkcu");
  const dir = machine ? `C:\\Program Files\\${name}` : `${HOME}\\AppData\\Local\\Programs\\${name}`;
  return {
    id,
    name,
    publisher,
    version: "1.2.3",
    source: "win32",
    scope: machine ? "machine" : "user",
    arch: machine ? "x64" : null,
    installLocation: dir,
    uninstall: { exe: `${dir}\\uninstall.exe`, args: "", raw: `"${dir}\\uninstall.exe"` },
    quietUninstall: null,
    icon: `${dir}\\${name.split(" ")[0]}.exe`,
    installedAt: now() - age * DAY,
    size,
    sizeMeasured: false,
    productCode: null,
    key: `HKEY_${machine ? "LOCAL_MACHINE" : "CURRENT_USER"}\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${name}`,
    package: null,
    url: null,
    caution: null,
    blocked: null,
    broken: false,
    ...extra,
  };
}

let programs: Program[] = [];
function reset(): void {
  programs = [
    prog("hklm:Steam", "Steam", "Valve Corporation", 42 * GB, 900),
    prog("hkcu:Discord", "Discord", "Discord Inc.", 410 * MB, 120),
    prog("hklm:Zalo", "Zalo", "VNG Corp", 780 * MB, 60),
    prog("hklm:{AC76BA86}", "Adobe Acrobat Reader", "Adobe", 1.1 * GB, 400, { source: "msi" }),
    prog("hklm:Old Game", "Old Game Launcher", "Retro Studio", 2.3 * GB, 1400, { broken: true }),
    prog("hklm:VC", "Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.38.33130", "Microsoft Corporation", 24 * MB, 700, { caution: "runtime" }),
    prog("hklm:NVIDIA", "NVIDIA Graphics Driver 551.23", "NVIDIA Corporation", 1.4 * GB, 200, { caution: "driver" }),
    prog("hkcu:NANOFreeDisk", "NANOFreeDisk", "NANO", 38 * MB, 3, { blocked: "self" }),
    prog("appx:Microsoft.WindowsCalculator", "Calculator", "Microsoft Corporation", 32 * MB, 30, {
      source: "store",
      scope: "user",
      installLocation: null,
      uninstall: null,
      key: null,
      package: { fullName: "Microsoft.WindowsCalculator_11.2401.0.0_x64__8wekyb3d8bbwe", family: "Microsoft.WindowsCalculator_8wekyb3d8bbwe" },
    }),
  ];
}
reset();

function left(id: string, kind: Leftover["kind"], reason: string, bytes: number, extra: Partial<Leftover> = {}): Leftover {
  const level = extra.level ?? "safe";
  const blocked = extra.blocked ?? null;
  return {
    id,
    kind,
    path: id.split("|")[0],
    value: id.includes("|") ? id.split("|")[1] : null,
    scope: id.startsWith("HKEY_LOCAL") || id.startsWith("C:\\Program") || id.startsWith("C:\\ProgramData") ? "machine" : "user",
    level,
    reason,
    detail: null,
    also: null,
    bytes,
    files: bytes ? Math.max(1, Math.round(bytes / (200 * 1024))) : 0,
    mtime: now() - 5 * DAY,
    selected: level === "safe" && !blocked && !extra.also,
    blocked,
    ...extra,
  };
}

function leftoversOf(p: Program): LeftoverReport {
  const n = p.name;
  const items: Leftover[] = [
    left(p.installLocation ?? `C:\\Program Files\\${n}`, "folder", "install-dir", 64 * MB),
    left(`${HOME}\\AppData\\Roaming\\${n}`, "folder", "same-name", 220 * MB),
    left(`${HOME}\\AppData\\Local\\${n}`, "folder", "same-name", 35 * MB),
    left(`C:\\ProgramData\\${n} Updater`, "folder", "weak-name", 3 * MB, { level: "caution" }),
    left(`${HOME}\\Desktop\\${n}.lnk`, "shortcut", "shortcut", 2048, { detail: `${p.installLocation}\\${n}.exe` }),
    left(`HKEY_CURRENT_USER\\Software\\${n}`, "registry-key", "software-key", 0),
    left(`HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\${n}`, "registry-key", "software-key", 0),
    left(`HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run|${n}`, "registry-value", "run-entry", 0),
    left(`service:${n}Svc`, "service", "service", 0, { path: `${p.installLocation}\\svc.exe`, detail: `${n}Service`, level: "caution", blocked: "info", selected: false }),
  ];
  return {
    program: p,
    mode: p.broken ? "broken" : "after-uninstall",
    items,
    selectedBytes: items.filter((i) => i.selected).reduce((s, i) => s + i.bytes, 0),
    totalBytes: items.reduce((s, i) => s + i.bytes, 0),
  };
}

let reports: Record<string, LeftoverReport> = {};

function junk(): JunkReport {
  const roaming = `${HOME}\\AppData\\Roaming`;
  const local = `${HOME}\\AppData\\Local`;
  const item = (path: string, kind: JunkGroup["items"][number]["kind"], what: string, bytes: number, level: "safe" | "caution" = "safe") => ({
    path,
    kind,
    what,
    level,
    isDir: !path.endsWith(".dmp") && !path.endsWith(".nupkg"),
    bytes,
    files: Math.max(1, Math.round(bytes / (64 * 1024))),
    newest: now() - 2 * DAY,
    size: 0,
    mtime: now() - 2 * DAY,
    selected: level === "safe",
    blocked: null,
  });
  const groups: JunkGroup[] = [
    {
      key: "hkcu:Discord",
      program: "hkcu:Discord",
      name: "Discord",
      publisher: "Discord Inc.",
      folder: `${roaming}\\discord`,
      requiresClosed: ["discord.exe"],
      running: true,
      items: [
        item(`${roaming}\\discord\\Cache`, "cache", "chromium-cache", 610 * MB),
        item(`${roaming}\\discord\\Code Cache`, "cache", "chromium-cache", 88 * MB),
        item(`${roaming}\\discord\\GPUCache`, "cache", "chromium-cache", 4 * MB),
        item(`${roaming}\\discord\\Crashpad\\reports`, "crash", "crashpad", 12 * MB),
        item(`${local}\\Discord\\app-1.0.9035`, "update", "old-version", 180 * MB, "caution"),
        item(`${local}\\Discord\\packages\\Discord-1.0.9035-full.nupkg`, "update", "update-package", 95 * MB),
      ],
      bytes: 0,
    },
    {
      key: "hklm:Zalo",
      program: "hklm:Zalo",
      name: "Zalo",
      publisher: "VNG Corp",
      folder: `${roaming}\\ZaloPC`,
      requiresClosed: ["zalo.exe"],
      running: false,
      items: [item(`${roaming}\\ZaloPC\\logs`, "log", "logs", 160 * MB, "caution"), item(`${roaming}\\ZaloPC\\crash.dmp`, "crash", "dump-file", 48 * MB)],
      bytes: 0,
    },
    {
      key: "folder:c:/users/nano/appdata/local/acme-updater",
      program: null,
      name: "acme-updater",
      publisher: null,
      folder: `${local}\\acme-updater`,
      requiresClosed: [],
      running: false,
      items: [item(`${local}\\acme-updater`, "update", "pending-update", 140 * MB)],
      bytes: 0,
    },
  ];
  for (const g of groups) g.bytes = g.items.reduce((s, i) => s + i.bytes, 0);
  const all = groups.flatMap((g) => g.items);
  return {
    groups,
    totalBytes: all.reduce((s, i) => s + i.bytes, 0),
    selectedBytes: all.filter((i) => i.selected).reduce((s, i) => s + i.bytes, 0),
    cancelled: false,
  };
}

let junkReport: JunkReport | null = null;

function op(freed: number, done: number): OperationResult {
  return { id: `mock-${Date.now()}`, freed, done, partial: 0, skipped: 0, failed: 0, cancelled: false, problems: [], journalError: null };
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const programsHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  programs_list: ({ store }) => programs.filter((p) => store || p.source !== "store"),
  programs_uninstall: async ({ id }) => {
    const p = programs.find((x) => x.id === id);
    if (!p) throw { code: "not-found", message: id };
    await wait(1200);
    reports[id] = leftoversOf(p);
    programs = programs.filter((x) => x.id !== id);
    return { id, status: p.broken ? "broken" : "removed", waited: p.broken ? null : "done", message: null };
  },
  programs_stop_waiting: () => undefined,
  programs_leftovers: ({ id }) => reports[id] ?? (() => {
    const p = programs.find((x) => x.id === id);
    if (!p) throw { code: "not-found", message: id };
    return (reports[id] = leftoversOf(p));
  })(),
  programs_leftovers_remove: ({ id, picked }) => {
    const r = reports[id];
    const chosen = (r?.items ?? []).filter((i) => picked.includes(i.id));
    delete reports[id];
    return op(chosen.reduce((s, i) => s + i.bytes, 0), chosen.length);
  },
  programs_junk_scan: () => (junkReport ??= junk()),
  programs_junk_clean: ({ paths }) => {
    const r = junkReport ?? junk();
    const freed = r.groups.flatMap((g) => g.items).filter((i) => paths.includes(i.path)).reduce((s, i) => s + i.bytes, 0);
    junkReport = {
      ...r,
      groups: r.groups.map((g) => ({ ...g, items: g.items.filter((i) => !paths.includes(i.path)) })).filter((g) => g.items.length),
    };
    return op(freed, paths.length);
  },
  programs_reveal: () => undefined,
  activity_restore_admin: ({ items }) => (items ?? []).map((n: number) => ({ n, path: "", status: "restored", message: null })),
};

/** Tests: start again from the sample programs. */
export function resetProgramsMock(): void {
  reset();
  reports = {};
  junkReport = null;
}
