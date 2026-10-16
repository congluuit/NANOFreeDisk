// File-type categories for the treemap/sunburst palette. Each category maps to a
// theme token (`--tm-1` … `--tm-8`, `--tm-other`) so themes control every color.

import { extOf } from "./format.ts";

export const CATEGORIES = [
  "video",
  "image",
  "audio",
  "archive",
  "code",
  "binary",
  "document",
  "data",
] as const;
export type Category = (typeof CATEGORIES)[number] | "other";

const MAP: Record<string, Category> = {};
const add = (cat: Category, exts: string) => exts.split(" ").forEach((e) => (MAP[e] = cat));
add("video", "mp4 mkv avi mov wmv webm m4v flv mpg mpeg ts m2ts vob 3gp");
add("image", "jpg jpeg png gif bmp tif tiff webp heic heif raw cr2 cr3 nef arw dng psd svg ico exr hdr tga dds");
add("audio", "mp3 flac wav ogg m4a aac wma opus aiff mid");
add("archive", "zip rar 7z tar gz tgz bz2 xz zst iso img cab msi msix appx nupkg whl jar apk dmg");
add("code", "rs ts tsx js jsx mjs py c h cpp hpp cc cs java kt go rb php swift lua sh ps1 bat cmd json toml yaml yml xml html css scss svelte vue md sql gradle cmake");
add("binary", "exe dll sys so dylib lib a o obj pdb ilk pch idb exp node wasm class pyc");
add("document", "pdf doc docx xls xlsx ppt pptx odt ods odp txt rtf epub csv one");
add("data", "db sqlite sqlite3 mdb ldf mdf vhd vhdx vmdk vdi qcow2 pak bin dat log etl evtx dmp cache blob uasset umap bundle assets resS");

export function categoryOf(name: string): Category {
  return MAP[extOf(name)] ?? "other";
}

export function tokenOf(cat: Category): string {
  const i = CATEGORIES.indexOf(cat as (typeof CATEGORIES)[number]);
  return i < 0 ? "--tm-other" : `--tm-${i + 1}`;
}

/** Resolve the current palette from CSS variables (call again after a theme change). */
export function readPalette(el: Element = document.documentElement): Record<Category, string> & {
  dir: string;
  rest: string;
  link: string;
  border: string;
  text: string;
  hover: string;
  selected: string;
} {
  const css = getComputedStyle(el);
  const v = (name: string) => css.getPropertyValue(name).trim() || "#888";
  const out: Record<string, string> = {};
  for (const c of CATEGORIES) out[c] = v(tokenOf(c));
  out.other = v("--tm-other");
  out.dir = v("--tm-dir");
  out.rest = v("--tm-rest");
  out.link = v("--tm-link");
  out.border = v("--tm-border");
  out.text = v("--tm-text");
  out.hover = v("--tm-hover");
  out.selected = v("--accent");
  return out as ReturnType<typeof readPalette>;
}
