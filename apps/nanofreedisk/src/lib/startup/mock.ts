// Browser-only stand-in for the Startup commands (`pnpm dev` without Tauri): a
// handful of entries from every source, one turned off, one whose program is
// gone, Windows Security's tray icon. Nothing touches a real registry.

import type { StartupEntry, StartupIcon } from "./types";

const HOME = "C:\\Users\\nano";
const RUN = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

function entry(origin: StartupEntry["origin"], name: string, command: string, exe: string | null, extra: Partial<StartupEntry> = {}): StartupEntry {
  const user = origin === "user-run" || origin === "user-folder";
  const folder = origin.endsWith("folder");
  const approvedName = folder ? `${name}.lnk` : name;
  return {
    id: `${origin}|${approvedName}`,
    name,
    command,
    exe,
    missing: false,
    publisher: null,
    origin,
    scope: user ? "user" : "machine",
    location: folder
      ? user
        ? `${HOME}\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup`
        : "C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs\\Startup"
      : `${user ? "HKEY_CURRENT_USER" : "HKEY_LOCAL_MACHINE"}\\${origin === "machine-run32" ? "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run" : RUN}`,
    enabled: true,
    disabledAt: null,
    caution: null,
    approvedName,
    ...extra,
  };
}

let entries: StartupEntry[] = [];
export function resetStartupMock(): void {
  const now = Math.floor(Date.now() / 1000);
  entries = [
    entry("user-run", "Discord", `"${HOME}\\AppData\\Local\\Discord\\Update.exe" --processStart Discord.exe`, `${HOME}\\AppData\\Local\\Discord\\Update.exe`, { publisher: "Discord Inc." }),
    entry("user-run", "Spotify", `"${HOME}\\AppData\\Roaming\\Spotify\\Spotify.exe" --autostart --minimized`, `${HOME}\\AppData\\Roaming\\Spotify\\Spotify.exe`, {
      publisher: "Spotify AB",
      enabled: false,
      disabledAt: now - 12 * 86_400,
    }),
    entry("user-run", "OneDrive", `"C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe" /background`, "C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe", { publisher: "Microsoft Corporation" }),
    entry("user-run", "OldUpdater", "C:\\Gone\\updater.exe /silent", "C:\\Gone\\updater.exe", { missing: true }),
    entry("user-run", "ctfmon", "C:\\Windows\\System32\\ctfmon.exe", "C:\\Windows\\System32\\ctfmon.exe", { publisher: "Microsoft Windows", caution: "windows" }),
    entry("user-folder", "Notes", `"C:\\Tools\\notes.exe" --tray`, "C:\\Tools\\notes.exe", { publisher: "Acme Tools *" }),
    entry("machine-run", "SecurityHealth", "%windir%\\system32\\SecurityHealthSystray.exe", "C:\\Windows\\system32\\SecurityHealthSystray.exe", {
      publisher: "Microsoft Windows",
      caution: "security",
    }),
    entry("machine-run", "RtkAudUService", `"C:\\Windows\\System32\\DriverStore\\FileRepository\\realtek\\RtkAudUService64.exe" -background`, "C:\\Windows\\System32\\DriverStore\\FileRepository\\realtek\\RtkAudUService64.exe", {
      publisher: "Realtek Semiconductor",
      caution: "windows",
    }),
    entry("machine-run32", "Steam", `"C:\\Program Files (x86)\\Steam\\steam.exe" -silent`, "C:\\Program Files (x86)\\Steam\\steam.exe", { publisher: "Valve Corp." }),
    entry("common-folder", "Zalo", `"C:\\Program Files\\Zalo\\Zalo.exe"`, "C:\\Program Files\\Zalo\\Zalo.exe", { publisher: "VNG Corp", enabled: false, disabledAt: now - 3 * 86_400 }),
  ];
}
resetStartupMock();

function icon(color: string, letter: string): StartupIcon {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect width="48" height="48" rx="10" fill="${color}"/><text x="24" y="32" font-size="24" font-family="sans-serif" text-anchor="middle" fill="#fff">${letter}</text></svg>`;
  return { src: `data:image/svg+xml;base64,${btoa(svg)}`, plate: false };
}

const ICONS: Record<string, StartupIcon> = {
  "user-run|Discord": icon("#5865f2", "D"),
  "user-run|Spotify": icon("#1db954", "S"),
  "machine-run32|Steam": icon("#1b2838", "S"),
};

export const startupHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  startup_list: () => entries.map((e) => ({ ...e })),
  startup_set: ({ id, enabled }) => {
    const e = entries.find((x) => x.id === id);
    if (!e) throw { code: "not-found", message: id };
    if (e.scope !== "user") throw { code: "needs-admin", message: id };
    e.enabled = !!enabled;
    e.disabledAt = enabled ? null : Math.floor(Date.now() / 1000);
    return { ...e };
  },
  startup_icons: ({ ids }) => Object.fromEntries((ids as string[]).filter((id) => ICONS[id]).map((id) => [id, ICONS[id]])),
  startup_open_task_manager: () => undefined,
};
