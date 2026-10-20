// Soft ("pastel") fills of the treemap, the legend and the sunburst, derived from a
// theme color so every theme and custom palette follows: a light tint of the color
// on the card in light themes, a deep one in dark themes, a stronger edge, and a
// label color that keeps WCAG AA (4.5:1) on the fill. Tested in theme.test.ts.

import { ensureContrast, isHex, mix } from "./theme.ts";

export interface TileStyle {
  /** Leaf tile. */
  fill: string;
  edge: string;
  ink: string;
  /** Folder frame (a fainter tint, so its children stand out). */
  frame: string;
  frameEdge: string;
  frameInk: string;
  /** Diagonal hatching of what is safe to clear. */
  hatch: string;
}

/** `#rrggbb` (a CSS value that is not one falls back to gray). */
export function hex6(v: string): string {
  const s = v.trim();
  if (isHex(s)) return s.toLowerCase();
  const short = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(s);
  return short ? `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}`.toLowerCase() : "#888888";
}

export function tileStyle(color: string, surface: string, dark: boolean): TileStyle {
  const c = hex6(color);
  const bg = hex6(surface);
  const fill = mix(c, bg, dark ? 0.3 : 0.2);
  const frame = mix(c, bg, dark ? 0.12 : 0.07);
  return {
    fill,
    edge: mix(c, bg, dark ? 0.5 : 0.42),
    ink: ensureContrast(c, fill, 4.5),
    frame,
    frameEdge: mix(c, bg, dark ? 0.34 : 0.3),
    frameInk: ensureContrast(c, frame, 4.5),
    hatch: mix(c, bg, dark ? 0.62 : 0.5),
  };
}
