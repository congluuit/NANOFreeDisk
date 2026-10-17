// Per-website browser cache panel: pure helpers over the scan report (tested in
// browsers.test.ts). A site row is one selection item per browser profile.

import type { SelItem } from "../selection";
import type { BrowserReport, ProfileReport, SiteRow, SitesReport, SiteTarget } from "./types";

/** Selection id of a site in a profile. */
export const siteId = (b: BrowserReport, p: ProfileReport, s: SiteRow): string => `${b.id}|${p.path}|${s.site}`;

/** Group id of a profile (its "select all" checkbox). */
export const profileGroup = (b: BrowserReport, p: ProfileReport): string => `${b.id}|${p.path}`;

/** `true` when `site` contains every word of `query` (case-insensitive). */
export function siteMatches(site: string, query: string): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return words.every((w) => site.includes(w));
}

/** Selection items: only what per-site cleaning removes counts; a site with nothing
 *  removable is locked, a running browser's sites are counted as skipped. */
export function siteItems(r: SitesReport | null, query = ""): SelItem[] {
  const out: SelItem[] = [];
  for (const b of r?.browsers ?? []) {
    for (const p of b.profiles) {
      for (const s of p.sites) {
        if (query && !siteMatches(s.site, query)) continue;
        out.push({
          id: siteId(b, p, s),
          bytes: s.cleanableBytes,
          files: s.cleanableEntries,
          level: "safe",
          group: profileGroup(b, p),
          locked: s.cleanableBytes === 0 && s.cleanableEntries === 0,
          skip: b.running ? b.exe : null,
        });
      }
    }
  }
  return out;
}

/** What the backend cleans for the picked ids (exact sites, one profile each). */
export function targetsOf(ids: string[]): SiteTarget[] {
  return ids.flatMap((id) => {
    const first = id.indexOf("|");
    const last = id.lastIndexOf("|");
    if (first < 0 || last <= first) return [];
    return [{ browser: id.slice(0, first), profile: id.slice(first + 1, last), site: id.slice(last + 1), subdomains: false }];
  });
}
