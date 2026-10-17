// Browser-only stand-in for the per-website browser cache commands (`pnpm dev`
// without Tauri): two browsers with a few sites.

import type { OperationResult } from "../clean/types";
import type { BrowserReport, SiteRow, SitesReport, SiteTarget } from "./types";

const MB = 2 ** 20;

function site(name: string, js: number, sw: number, http: number): SiteRow {
  return {
    site: name,
    bytes: (js + sw + http) * MB,
    entries: Math.round(js * 3 + sw + http * 4),
    cleanableBytes: (js + sw) * MB,
    cleanableEntries: Math.round(js * 3 + sw),
    listedBytes: http * MB,
    stores: [...(http ? ["http-cache" as const] : []), ...(js ? ["code-cache-js" as const] : []), ...(sw ? ["cache-storage" as const] : [])],
  };
}

let report: SitesReport = {
  truncated: false,
  browsers: [
    {
      id: "chrome",
      name: "Chrome",
      ruleId: "browser.chrome.cache",
      exe: "chrome.exe",
      running: false,
      profiles: [
        {
          name: "Default",
          path: "C:\Users\demo\AppData\Local\Google\Chrome\User Data\Default",
          stores: [],
          sites: [site("mail.google.com", 0, 106, 0), site("youtube.com", 45, 2, 8), site("github.com", 12, 0, 20), site("example.com", 0.4, 0, 1)],
        },
      ],
    },
    {
      id: "edge",
      name: "Edge",
      ruleId: "browser.edge.cache",
      exe: "msedge.exe",
      running: true,
      profiles: [
        {
          name: "Default",
          path: "C:\Users\demo\AppData\Local\Microsoft\Edge\User Data\Default",
          stores: [],
          sites: [site("bing.com", 0.5, 0, 24), site("ntp.msn.com", 1, 20, 0)],
        },
      ],
    },
  ] as BrowserReport[],
};

function picked(targets: SiteTarget[]): SiteRow[] {
  return report.browsers.flatMap((b) =>
    b.running ? [] : b.profiles.flatMap((p) => p.sites.filter((s) => targets.some((t) => t.browser === b.id && t.profile === p.path && t.site === s.site))),
  );
}

export const browsersHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  browsers_sites: () => structuredClone(report),
  browsers_plan: (a) => {
    const rows = picked(a.targets);
    const running = report.browsers.filter((b) => b.running && a.targets.some((t: SiteTarget) => t.browser === b.id)).map((b) => b.exe);
    return {
      items: rows.reduce((n, s) => n + s.cleanableEntries, 0),
      entries: rows.reduce((n, s) => n + s.cleanableEntries, 0),
      bytes: rows.reduce((n, s) => n + s.cleanableBytes, 0),
      byStore: [{ kind: "code-cache-js", entries: rows.reduce((n, s) => n + s.cleanableEntries, 0), bytes: rows.reduce((n, s) => n + s.cleanableBytes, 0) }],
      indexFiles: rows.length ? 1 : 0,
      listedOnly: rows.reduce((n, s) => n + s.listedBytes, 0),
      running,
      refused: 0,
    };
  },
  browsers_clean: (a): OperationResult => {
    const rows = picked(a.targets);
    const freed = rows.reduce((n, s) => n + s.cleanableBytes, 0);
    report = {
      ...report,
      browsers: report.browsers.map((b) => ({
        ...b,
        profiles: b.profiles.map((p) => ({ ...p, sites: p.sites.map((s) => (rows.includes(s) ? { ...s, bytes: s.listedBytes, cleanableBytes: 0, cleanableEntries: 0 } : s)) })),
      })),
    };
    return { id: `mock-${Date.now()}`, freed, done: rows.length, partial: 0, skipped: 0, failed: 0, cancelled: false, problems: [], journalError: null };
  },
  browsers_cancel: () => undefined,
};
