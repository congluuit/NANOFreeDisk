// Design tokens and themes (PLAN §11): Light / Dark / System / Custom, presets,
// theme.toml import/export and a WCAG contrast check. Every color the UI paints —
// CSS and canvas — comes from these tokens.

export const EDITABLE_TOKENS = [
  "bg",
  "surface",
  "surface-2",
  "border",
  "text",
  "text-muted",
  "accent",
  "accent-contrast",
  "danger",
  "risk-safe",
  "risk-caution",
  "risk-advanced",
  "risk-protected",
  "tm-1",
  "tm-2",
  "tm-3",
  "tm-4",
  "tm-5",
  "tm-6",
  "tm-7",
  "tm-8",
  "tm-other",
  "tm-dir",
] as const;

export type Token = (typeof EDITABLE_TOKENS)[number];
export type Palette = Record<Token, string>;
export type ThemeMode = "light" | "dark" | "system" | "custom";
export type PresetId = "nano-dark" | "nano-light" | "nord" | "solarized" | "high-contrast";

export const PRESETS: Record<PresetId, { label: string; dark: boolean; colors: Palette }> = {
  "nano-dark": {
    label: "NANO Dark",
    dark: true,
    colors: {
      bg: "#0d1117",
      surface: "#151b24",
      "surface-2": "#1c2430",
      border: "#29333f",
      text: "#e8edf4",
      "text-muted": "#9aa5b6",
      accent: "#22d3ee",
      "accent-contrast": "#04121a",
      danger: "#f87171",
      "risk-safe": "#22c55e",
      "risk-caution": "#eab308",
      "risk-advanced": "#f97316",
      "risk-protected": "#ef4444",
      "tm-1": "#f472b6",
      "tm-2": "#a78bfa",
      "tm-3": "#60a5fa",
      "tm-4": "#f59e0b",
      "tm-5": "#22d3ee",
      "tm-6": "#a3e635",
      "tm-7": "#facc15",
      "tm-8": "#2dd4bf",
      "tm-other": "#64748b",
      "tm-dir": "#19212c",
    },
  },
  "nano-light": {
    label: "NANO Light",
    dark: false,
    colors: {
      bg: "#f4f5f7",
      surface: "#ffffff",
      "surface-2": "#f0f2f5",
      border: "#e2e5ea",
      text: "#111827",
      "text-muted": "#5b6474",
      accent: "#0e7490",
      "accent-contrast": "#ffffff",
      danger: "#dc2626",
      "risk-safe": "#16a34a",
      "risk-caution": "#ca8a04",
      "risk-advanced": "#ea580c",
      "risk-protected": "#dc2626",
      "tm-1": "#ec4899",
      "tm-2": "#8b5cf6",
      "tm-3": "#3b82f6",
      "tm-4": "#f59e0b",
      "tm-5": "#06b6d4",
      "tm-6": "#84cc16",
      "tm-7": "#eab308",
      "tm-8": "#14b8a6",
      "tm-other": "#94a3b8",
      "tm-dir": "#eef1f5",
    },
  },
  nord: {
    label: "Nord",
    dark: true,
    colors: {
      bg: "#2e3440",
      surface: "#3b4252",
      "surface-2": "#434c5e",
      border: "#4c566a",
      text: "#eceff4",
      "text-muted": "#c0c8d6",
      accent: "#88c0d0",
      "accent-contrast": "#2e3440",
      danger: "#bf616a",
      "risk-safe": "#a3be8c",
      "risk-caution": "#ebcb8b",
      "risk-advanced": "#d08770",
      "risk-protected": "#bf616a",
      "tm-1": "#b48ead",
      "tm-2": "#81a1c1",
      "tm-3": "#5e81ac",
      "tm-4": "#ebcb8b",
      "tm-5": "#88c0d0",
      "tm-6": "#a3be8c",
      "tm-7": "#d08770",
      "tm-8": "#8fbcbb",
      "tm-other": "#616e88",
      "tm-dir": "#353c4a",
    },
  },
  solarized: {
    label: "Solarized Light",
    dark: false,
    colors: {
      bg: "#fdf6e3",
      surface: "#fffbee",
      "surface-2": "#eee8d5",
      border: "#d9cfb3",
      text: "#073642",
      "text-muted": "#4e636b",
      accent: "#1f6fae",
      "accent-contrast": "#ffffff",
      danger: "#dc322f",
      "risk-safe": "#859900",
      "risk-caution": "#b58900",
      "risk-advanced": "#cb4b16",
      "risk-protected": "#dc322f",
      "tm-1": "#d33682",
      "tm-2": "#6c71c4",
      "tm-3": "#268bd2",
      "tm-4": "#b58900",
      "tm-5": "#2aa198",
      "tm-6": "#859900",
      "tm-7": "#cb4b16",
      "tm-8": "#93a1a1",
      "tm-other": "#a8a497",
      "tm-dir": "#efe8d3",
    },
  },
  "high-contrast": {
    label: "High contrast",
    dark: true,
    colors: {
      bg: "#000000",
      surface: "#000000",
      "surface-2": "#1a1a1a",
      border: "#ffffff",
      text: "#ffffff",
      "text-muted": "#e6e6e6",
      accent: "#ffff00",
      "accent-contrast": "#000000",
      danger: "#ff5c5c",
      "risk-safe": "#00ff66",
      "risk-caution": "#ffff00",
      "risk-advanced": "#ff9900",
      "risk-protected": "#ff3333",
      "tm-1": "#ff00ff",
      "tm-2": "#00ffff",
      "tm-3": "#3399ff",
      "tm-4": "#ffff00",
      "tm-5": "#00ff99",
      "tm-6": "#99ff00",
      "tm-7": "#ff9900",
      "tm-8": "#cccccc",
      "tm-other": "#808080",
      "tm-dir": "#000000",
    },
  },
};

export interface ThemeSettings {
  mode: ThemeMode;
  /** Base of the Custom mode. */
  preset: PresetId;
  /** Custom-mode overrides on top of the preset. */
  overrides: Partial<Palette>;
}

export const DEFAULT_THEME: ThemeSettings = { mode: "system", preset: "nano-dark", overrides: {} };

/** Resolve the palette for the current settings and environment. */
export function resolvePalette(
  s: ThemeSettings,
  systemDark: boolean,
  systemAccent: string | null,
): { colors: Palette; dark: boolean } {
  switch (s.mode) {
    case "light":
      return { colors: { ...PRESETS["nano-light"].colors }, dark: false };
    case "dark":
      return { colors: { ...PRESETS["nano-dark"].colors }, dark: true };
    case "system": {
      const base = PRESETS[systemDark ? "nano-dark" : "nano-light"];
      const colors = { ...base.colors };
      if (systemAccent && isHex(systemAccent)) {
        // keep the Windows hue but make sure it reads on this background
        colors.accent = ensureContrast(systemAccent, colors.surface, 3);
        colors["accent-contrast"] = bestText(colors.accent);
      }
      return { colors, dark: systemDark };
    }
    case "custom": {
      const base = PRESETS[s.preset] ?? PRESETS["nano-dark"];
      const colors = { ...base.colors };
      for (const [k, v] of Object.entries(s.overrides)) {
        if ((EDITABLE_TOKENS as readonly string[]).includes(k) && isHex(v)) colors[k as Token] = v!;
      }
      return { colors, dark: luminance(colors.bg) < 0.4 };
    }
  }
}

/** The palette plus derived tokens, keyed by CSS variable name without `--`. */
export function paletteVars(colors: Palette, dark: boolean): Record<string, string> {
  const all: Record<string, string> = { ...colors };
  all["accent-soft"] = mix(colors.accent, colors.surface, 0.18);
  all["danger-soft"] = mix(colors.danger, colors.surface, 0.15);
  all["hover"] = mix(colors.text, colors.surface, 0.06);
  // soft fills and readable text of the risk colors (pills, stat dots, hatching)
  for (const r of ["risk-safe", "risk-caution", "risk-advanced", "risk-protected"] as const) {
    const soft = mix(colors[r], colors.surface, dark ? 0.2 : 0.13);
    all[`${r}-soft`] = soft;
    all[`${r}-ink`] = ensureContrast(colors[r], soft, 4.5);
  }
  all["accent-ink"] = ensureContrast(colors.accent, all["accent-soft"], 4.5);
  all["tm-rest"] = mix(colors["tm-other"], colors.bg, 0.45);
  all["tm-link"] = mix(colors["text-muted"], colors.bg, 0.35);
  all["tm-border"] = dark ? mix("#000000", colors.bg, 0.35) : mix("#ffffff", colors.bg, 0.6);
  all["tm-text"] = colors.text;
  all["tm-hover"] = dark ? "#ffffff" : "#000000";
  // Hardware sensors: temperature levels follow the risk colors so every theme
  // (and Custom overrides) keeps them readable
  all["temp-ok"] = colors["risk-safe"];
  all["temp-warm"] = colors["risk-caution"];
  all["temp-hot"] = colors["risk-protected"];
  // Monitor: read / write series and bars
  all["io-read"] = colors["tm-3"];
  all["io-write"] = colors["risk-advanced"];
  all["shadow"] = dark ? "0 8px 30px rgba(0,0,0,.45)" : "0 8px 30px rgba(15,23,42,.12)";
  all["shadow-sm"] = dark ? "0 1px 2px rgba(0,0,0,.4)" : "0 1px 2px rgba(15,23,42,.06)";
  return all;
}

/** Write the palette (plus derived tokens) onto `:root` as CSS variables. */
export function applyPalette(colors: Palette, dark: boolean, root: HTMLElement = document.documentElement): void {
  for (const [k, v] of Object.entries(paletteVars(colors, dark))) root.style.setProperty(`--${k}`, v);
  root.dataset.theme = dark ? "dark" : "light";
  root.style.colorScheme = dark ? "dark" : "light";
}

// ── color math ───────────────────────────────────────────────────────────────

export function isHex(v: unknown): v is string {
  return typeof v === "string" && /^#[0-9a-fA-F]{6}$/.test(v);
}

function rgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function hex([r, g, b]: number[]): string {
  return "#" + [r, g, b].map((c) => Math.round(Math.max(0, Math.min(255, c))).toString(16).padStart(2, "0")).join("");
}

/** `amount` of `a` over `b` (0 → b, 1 → a). */
export function mix(a: string, b: string, amount: number): string {
  const x = rgb(a);
  const y = rgb(b);
  return hex(x.map((c, i) => c * amount + y[i] * (1 - amount)));
}

export function luminance(color: string): number {
  const [r, g, b] = rgb(color).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

export function bestText(bg: string): string {
  return contrast("#ffffff", bg) >= contrast("#0b1220", bg) ? "#ffffff" : "#0b1220";
}

/** Darken/lighten `color` towards black/white until it reaches `ratio` against `bg`. */
export function ensureContrast(color: string, bg: string, ratio: number): string {
  const target = luminance(bg) > 0.5 ? "#000000" : "#ffffff";
  let c = color;
  for (let i = 0; i < 20 && contrast(c, bg) < ratio; i++) c = mix(target, c, 0.1);
  return c;
}

export interface ContrastIssue {
  fg: Token;
  bg: Token;
  ratio: number;
}

/** Token pairs below WCAG AA (4.5:1 for text). */
export function contrastIssues(colors: Palette): ContrastIssue[] {
  const pairs: [Token, Token][] = [
    ["text", "bg"],
    ["text", "surface"],
    ["text-muted", "surface"],
    ["accent-contrast", "accent"],
  ];
  return pairs
    .map(([fg, bg]) => ({ fg, bg, ratio: contrast(colors[fg], colors[bg]) }))
    .filter((p) => p.ratio < 4.5);
}

// ── theme.toml ───────────────────────────────────────────────────────────────

export function exportToml(name: string, preset: PresetId, colors: Palette): string {
  const lines = [
    "# NANOFreeDisk theme — import it in Settings → Theme.",
    `name = ${JSON.stringify(name)}`,
    `base = ${JSON.stringify(preset)}`,
    "",
    "[colors]",
    ...EDITABLE_TOKENS.map((k) => `${k.includes("-") ? JSON.stringify(k) : k} = "${colors[k]}"`),
    "",
  ];
  return lines.join("\n");
}

/** Parse a theme file; `null` when it has no valid colors. Unknown keys are ignored. */
export function importToml(text: string): { name: string; preset: PresetId; overrides: Partial<Palette> } | null {
  let section = "";
  let name = "Custom";
  let preset: PresetId = "nano-dark";
  const overrides: Partial<Palette> = {};
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.replace(/\s+#.*$/, "").trim();
    if (!line || line.startsWith("#")) continue;
    const sec = /^\[([\w.-]+)\]$/.exec(line);
    if (sec) {
      section = sec[1];
      continue;
    }
    const kv = /^"?([\w-]+)"?\s*=\s*"([^"]*)"$/.exec(line);
    if (!kv) continue;
    const [, key, value] = kv;
    if (section === "" && key === "name") name = value;
    else if (section === "" && key === "base" && value in PRESETS) preset = value as PresetId;
    else if (section === "colors" && (EDITABLE_TOKENS as readonly string[]).includes(key) && isHex(value))
      overrides[key as Token] = value.toLowerCase();
  }
  return Object.keys(overrides).length ? { name, preset, overrides } : null;
}
