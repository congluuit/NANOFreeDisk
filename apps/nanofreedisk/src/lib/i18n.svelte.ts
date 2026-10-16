// Fluent-based i18n (PLAN §11: EN + VI mandatory). Message files live in the repo's
// top-level `locales/` so the CLI and installer can share them later.

import { FluentBundle, FluentResource, type FluentVariable } from "@fluent/bundle";
import en from "$locales/en/app.ftl?raw";
import vi from "$locales/vi/app.ftl?raw";

export type Locale = "en" | "vi";
export const LOCALES: { id: Locale; label: string }[] = [
  { id: "en", label: "English" },
  { id: "vi", label: "Tiếng Việt" },
];

function bundle(locale: Locale, source: string): FluentBundle {
  const b = new FluentBundle(locale, { useIsolating: false });
  const errors = b.addResource(new FluentResource(source));
  if (errors.length) console.warn(`fluent ${locale}:`, errors);
  return b;
}

const bundles: Record<Locale, FluentBundle> = { en: bundle("en", en), vi: bundle("vi", vi) };

export function systemLocale(): Locale {
  const langs = typeof navigator !== "undefined" ? navigator.languages ?? [navigator.language] : [];
  return langs.some((l) => l?.toLowerCase().startsWith("vi")) ? "vi" : "en";
}

export const i18n = $state<{ locale: Locale }>({ locale: systemLocale() });

/** Translate `id`; falls back to English, then to the id itself. */
export function t(id: string, args?: Record<string, FluentVariable>): string {
  for (const loc of [i18n.locale, "en"] as Locale[]) {
    const b = bundles[loc];
    const msg = b.getMessage(id);
    if (msg?.value) return b.formatPattern(msg.value, args);
  }
  return id;
}

/** BCP-47 tag for Intl formatting. */
export function intlLocale(): string {
  return i18n.locale === "vi" ? "vi-VN" : "en-US";
}
