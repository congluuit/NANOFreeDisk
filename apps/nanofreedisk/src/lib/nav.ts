// What the sidebar shows in each mode, and the Tools page catalog. Pure (tested in
// nav.test.ts): App.svelte, Tools.svelte and the command palette read it.
//
// Simple mode keeps the sidebar short: Home, Quick clean, the disk map, one Tools
// entry, the cleanup history and Settings. Expert mode keeps the full grouped list
// (plus Quick clean). Every view stays reachable in both modes: through Tools, the
// command palette (Ctrl+K) and the links between views.

import type { Mode, View } from "./state.svelte";

export interface NavItem {
  id: View;
  icon: string;
  /** Locale key of the label. */
  label: string;
}

export interface NavGroup {
  group: string;
  icon: string;
  label: string;
  items: NavItem[];
}

export type NavEntry = NavItem | NavGroup;

export const isGroup = (entry: NavEntry): entry is NavGroup => "group" in entry;

/** Expert mode: grouped by what the user wants to do. */
export const EXPERT_NAV: NavEntry[] = [
  { id: "home", icon: "home", label: "nav-home" },
  { id: "quick", icon: "clean", label: "nav-quick" },
  { id: "analyze", icon: "analyze", label: "nav-analyze" },
  {
    group: "free",
    icon: "clean",
    label: "nav-group-free",
    items: [
      { id: "clean", icon: "trash", label: "nav-clean" },
      { id: "devcache", icon: "devcache", label: "nav-devcache" },
      { id: "duplicates", icon: "duplicates", label: "nav-duplicates" },
      { id: "programs", icon: "apps", label: "nav-programs" },
      { id: "oldlarge", icon: "drive", label: "nav-oldlarge" },
      { id: "tidy", icon: "folder", label: "nav-tidy" },
    ],
  },
  {
    group: "pc",
    icon: "health",
    label: "nav-group-pc",
    items: [
      { id: "health", icon: "health", label: "nav-health" },
      { id: "hardware", icon: "cpu", label: "nav-hardware" },
      { id: "monitor", icon: "monitor", label: "nav-monitor" },
      { id: "startup", icon: "power", label: "nav-startup" },
    ],
  },
];

/** Simple mode: the few places a beginner needs. */
export const SIMPLE_NAV: NavEntry[] = [
  { id: "home", icon: "home", label: "nav-home" },
  { id: "quick", icon: "clean", label: "nav-quick" },
  { id: "analyze", icon: "analyze", label: "nav-diskmap" },
  { id: "tools", icon: "grid", label: "nav-tools" },
];

/** One card of the Tools page: when to use the tool, in one sentence. */
export interface ToolCard {
  id: View;
  icon: string;
  label: string;
  /** Locale key of the "use it when" sentence. */
  when: string;
  /** Section of the page. */
  section: "space" | "health";
}

export const TOOLS: ToolCard[] = [
  { id: "clean", icon: "trash", label: "nav-clean", when: "tools-when-clean", section: "space" },
  { id: "devcache", icon: "devcache", label: "nav-devcache", when: "tools-when-devcache", section: "space" },
  { id: "duplicates", icon: "duplicates", label: "nav-duplicates", when: "tools-when-duplicates", section: "space" },
  { id: "programs", icon: "apps", label: "nav-programs", when: "tools-when-programs", section: "space" },
  { id: "oldlarge", icon: "drive", label: "nav-oldlarge", when: "tools-when-oldlarge", section: "space" },
  { id: "tidy", icon: "folder", label: "nav-tidy", when: "tools-when-tidy", section: "space" },
  { id: "startup", icon: "power", label: "nav-startup-apps", when: "tools-when-startup", section: "health" },
  { id: "health", icon: "health", label: "nav-health", when: "tools-when-health", section: "health" },
  { id: "hardware", icon: "cpu", label: "nav-hardware", when: "tools-when-hardware", section: "health" },
  { id: "monitor", icon: "monitor", label: "nav-monitor", when: "tools-when-monitor", section: "health" },
];

/** The sidebar of `mode`, without the views `available` rejects (empty groups go). */
export function sidebar(mode: Mode, available: (view: View) => boolean): NavEntry[] {
  return (mode === "expert" ? EXPERT_NAV : SIMPLE_NAV).flatMap((entry): NavEntry[] => {
    if (!isGroup(entry)) return available(entry.id) ? [entry] : [];
    const items = entry.items.filter((item) => available(item.id));
    return items.length ? [{ ...entry, items }] : [];
  });
}

/** Tool cards of this platform. */
export function toolCards(available: (view: View) => boolean): ToolCard[] {
  return TOOLS.filter((c) => available(c.id));
}

/** The sidebar entry that stands for `view` (Simple mode: a tool lights up Tools). */
export function activeEntry(mode: Mode, view: View): View {
  if (mode === "expert") return view;
  return TOOLS.some((c) => c.id === view) ? "tools" : view;
}
