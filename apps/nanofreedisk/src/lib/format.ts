// Number/size/date formatting. Sizes use binary multiples with the labels Windows
// Explorer shows ("1.5 GB" = 1.5 × 2³⁰ bytes) so numbers match what users see there.

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function formatSize(bytes: number, locale = "en"): string {
  if (!Number.isFinite(bytes) || bytes < 1024) return `${Math.max(0, Math.round(bytes || 0))} B`;
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < UNITS.length - 1) {
    v /= 1024;
    u++;
  }
  const digits = v >= 100 ? 0 : 1;
  return `${v.toLocaleString(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits })} ${UNITS[u]}`;
}

export function formatCount(n: number, locale = "en"): string {
  return Math.round(n).toLocaleString(locale);
}

export function formatPercent(ratio: number, locale = "en"): string {
  return ratio.toLocaleString(locale, { style: "percent", maximumFractionDigits: ratio < 0.1 ? 1 : 0 });
}

export function formatDate(unixSeconds: number, locale = "en"): string {
  if (!unixSeconds) return "—";
  return new Date(unixSeconds * 1000).toLocaleDateString(locale, { year: "numeric", month: "short", day: "numeric" });
}

export function formatDuration(ms: number, locale = "en"): string {
  const s = ms / 1000;
  if (s < 60) return `${s.toLocaleString(locale, { maximumFractionDigits: s < 1 ? 2 : 1 })} s`;
  const m = Math.floor(s / 60);
  return `${m} min ${Math.round(s % 60)} s`;
}

/** Parse "10", "512K", "10MB", "1.5 GB" into bytes (binary units); `null` if invalid. */
export function parseSize(input: string): number | null {
  const m = /^\s*(\d+(?:\.\d+)?)\s*([kmgt]?)(?:i?b)?\s*$/i.exec(input);
  if (!m) return null;
  const shift = { "": 0, k: 10, m: 20, g: 30, t: 40 }[m[2].toLowerCase() as "" | "k" | "m" | "g" | "t"];
  return Math.round(parseFloat(m[1]) * 2 ** shift);
}

export function extOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

const TEMPLATE_VARS: Record<string, string> = {
  HOME: "%USERPROFILE%",
  USERPROFILE: "%USERPROFILE%",
  APPDATA: "%APPDATA%",
  LOCALAPPDATA: "%LOCALAPPDATA%",
  PROGRAMDATA: "%ProgramData%",
  PROGRAMFILES: "%ProgramFiles%",
  PROGRAMFILES_X86: "%ProgramFiles(x86)%",
  TEMP: "%TEMP%",
  SYSTEMROOT: "%SystemRoot%",
  SYSTEMDRIVE: "%SystemDrive%",
};

/** Show a protected-path template the way Windows users write paths. */
export function templateLabel(template: string | null): string {
  if (!template) return "—";
  return template
    .replace(/\$\{(\w+)\}/g, (_, v: string) => TEMPLATE_VARS[v] ?? v)
    .replace("<volume>", "X:")
    .replace(/\//g, "\\");
}

/** Size change with an explicit sign: "+1.5 GB", "−200 MB", "0 B". */
export function formatDelta(bytes: number, locale = "en"): string {
  const size = formatSize(Math.abs(bytes), locale);
  return bytes > 0 ? `+${size}` : bytes < 0 ? `−${size}` : size;
}

export function formatDateTime(unixSeconds: number, locale = "en"): string {
  if (!unixSeconds) return "—";
  return new Date(unixSeconds * 1000).toLocaleString(locale, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Whole days from `from` to `to` (Unix seconds), never negative. */
export function daysBetween(from: number, to: number): number {
  return Math.max(0, Math.round((to - from) / 86_400));
}
