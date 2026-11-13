// Browser-only stand-in for "Similar photos" (`pnpm dev` without Tauri): a phone
// photo sent through Zalo and Messenger, two screenshots, an edited copy in
// OneDrive, a cropped photo and a burst. Removing "frees" the bytes; nothing
// touches a real disk.

import type { OperationResult } from "../clean/types";
import type { CopyFlags } from "../dupes/types";
import type { Photo, Preset, SimilarGroup, SimilarReport, SimilarSelection } from "./types";

const KB = 1024;
const MB = 2 ** 20;
const DAY = 86_400;
const now = () => Math.floor(Date.now() / 1000);
const HOME = "C:\\Users\\nano";
const PLAIN: CopyFlags = { cloud: false, git: false, appDir: false, readonly: false };
const LIMIT: Record<Preset, number> = { strict: 40, normal: 64, loose: 96 };

function photo(path: string, width: number, height: number, size: number, age: number, distance: number, extra: Partial<Photo> = {}): Photo {
  return { path, width, height, size, mtime: now() - age * DAY, volume: "00000000c0c0c0c0", fileId: null, flags: PLAIN, distance, ...extra };
}

/** Every look-alike set the mock knows, with the distance its farthest photo needs. */
function library(): SimilarGroup[] {
  const cam = `${HOME}\\Pictures\\Camera\\IMG_20240812_101530.jpg`;
  const zalo = `${HOME}\\Documents\\Zalo Received Files\\z5432109876543_8f2a.jpg`;
  return [
    {
      id: 0,
      extra: 0,
      photos: [
        photo(cam, 4000, 3000, 3.8 * MB, 460, 0),
        photo(`${HOME}\\Downloads\\received_1029384756.jpeg`, 2048, 1536, 480 * KB, 455, 9),
        photo(zalo, 1280, 960, 210 * KB, 450, 14),
        photo(`${HOME}\\Downloads\\z5432109876543_8f2a.jpg`, 1280, 960, 210 * KB, 300, 14, { identicalTo: zalo }),
      ],
    },
    {
      id: 0,
      extra: 0,
      photos: [
        photo(`${HOME}\\Pictures\\Holiday\\DSC_0042.jpg`, 6000, 4000, 8.2 * MB, 900, 0),
        photo(`${HOME}\\OneDrive\\Pictures\\DSC_0042-edited.jpg`, 5800, 3866, 2.1 * MB, 880, 31, { flags: { ...PLAIN, cloud: true } }),
      ],
    },
    {
      id: 0,
      extra: 0,
      photos: [
        photo(`${HOME}\\Pictures\\Screenshots\\Screenshot 2024-05-02 093011.png`, 1920, 1080, 910 * KB, 120, 0),
        photo(`${HOME}\\Desktop\\Screenshot (1).png`, 1920, 1080, 880 * KB, 118, 22),
      ],
    },
    {
      id: 0,
      extra: 0,
      photos: [
        photo(`${HOME}\\Pictures\\2023\\IMG_0901.jpg`, 4032, 3024, 4.4 * MB, 700, 0),
        photo(`${HOME}\\Pictures\\2023\\IMG_0901 (cropped).jpg`, 3800, 2850, 3.9 * MB, 690, 52),
      ],
    },
    {
      id: 0,
      extra: 0,
      photos: [
        photo(`${HOME}\\Pictures\\2023\\IMG_1201.jpg`, 4032, 3024, 4.1 * MB, 650, 0),
        photo(`${HOME}\\Pictures\\2023\\IMG_1202.jpg`, 4032, 3024, 4.0 * MB, 650, 78),
      ],
    },
  ];
}

let all: SimilarGroup[] = library();
let preset: Preset = "strict";
let roots: string[] = [];

function groups(): SimilarGroup[] {
  const out = all
    .filter((g) => g.photos.length > 1 && Math.max(...g.photos.map((p) => p.distance)) <= LIMIT[preset])
    .map((g) => ({ ...g, extra: g.photos.slice(1).filter((p) => !p.identicalTo).reduce((s, p) => s + p.size, 0) }));
  out.sort((a, b) => b.extra - a.extra);
  out.forEach((g, i) => (g.id = i + 1));
  return out;
}

function report(): SimilarReport {
  const gs = groups();
  return {
    roots,
    minSize: 10 * KB,
    preset,
    groups: gs,
    extraBytes: gs.reduce((s, g) => s + g.extra, 0),
    scannedFiles: 48_210,
    images: 6_412,
    decoded: 6_380,
    cacheHits: 32,
    skipped: { small: 1_904, cloud: 37, unreadable: 3, plain: 12, "too-large": 1, "app-dir": 840 },
    skippedSamples: [
      { path: `${HOME}\\OneDrive\\Pictures\\Album 2018\\IMG_2231.jpg`, reason: "cloud" },
      { path: `${HOME}\\Downloads\\banner-cut.jpg`, reason: "unreadable" },
      { path: `${HOME}\\Pictures\\white.png`, reason: "plain" },
    ],
    elapsedMs: 41_000,
    totalGroups: gs.length,
  };
}

/** A stand-in picture: a few shapes in the group's colours. */
function thumb(path: string): string | null {
  const g = all.findIndex((x) => x.photos.some((p) => p.path === path));
  if (g < 0) return null;
  const p = all[g].photos.find((x) => x.path === path)!;
  const hue = (g * 67) % 360;
  const shift = p.distance / 4;
  const w = 160;
  const h = Math.round((w * p.height) / p.width);
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">` +
    `<rect width="${w}" height="${h}" fill="hsl(${hue},45%,${70 - shift}%)"/>` +
    `<circle cx="${w * 0.35 + shift}" cy="${h * 0.45}" r="${h * 0.28}" fill="hsl(${(hue + 40) % 360},55%,45%)"/>` +
    `<rect x="${w * 0.55}" y="${h * 0.55}" width="${w * 0.35}" height="${h * 0.3}" fill="hsl(${(hue + 180) % 360},40%,35%)"/>` +
    `</svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** What the one-click search looks through (`nfd_dupes::personal::personal_folders`). */
const PERSONAL = ["Documents", "Downloads", "Pictures", "Videos", "Music", "Desktop"].map((n) => `${HOME}\\${n}`);

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const similarHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  similar_find: ({ paths, options }) => {
    all = library();
    preset = (options?.preset as Preset) ?? "strict";
    roots = options?.personal ? PERSONAL : paths?.length ? paths : ["C:\\"];
    return report();
  },
  similar_regroup: ({ preset: p }) => {
    preset = p as Preset;
    return report();
  },
  similar_cancel: () => undefined,
  similar_report: () => (roots.length ? report() : null),
  similar_run: ({ selections }) => {
    const shown = groups();
    let freed = 0;
    let done = 0;
    for (const s of selections as SimilarSelection[]) {
      const g = shown.find((x) => x.id === s.group);
      if (!g) continue;
      if (s.remove.length >= g.photos.length && !s.keepNone) throw { code: "similar-plan", message: `group ${s.group} would lose every photo: one must stay` };
      for (const path of s.remove) {
        const p = g.photos.find((x) => x.path === path);
        if (!p) continue;
        freed += p.size;
        done++;
        for (const lib of all) lib.photos = lib.photos.filter((x) => x.path !== path);
      }
    }
    const r: OperationResult = { id: `mock-similar-${now()}`, freed, done, partial: 0, skipped: 0, failed: 0, cancelled: false, problems: [], journalError: null };
    return r;
  },
  similar_reveal: () => undefined,
  similar_thumbnail: ({ path }) => thumb(path),
};
