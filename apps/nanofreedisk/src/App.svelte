<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "./lib/api";
  import { t } from "./lib/i18n.svelte";
  import {
    app,
    applyLocale,
    listenScanEvents,
    loadSettings,
    openStartupPath,
    refreshVolumes,
    saveSettingsSoon,
    type View,
  } from "./lib/state.svelte";
  import { applyPalette, resolvePalette } from "./lib/theme";
  import { startThermalSync } from "./lib/hw/thermal.svelte";
  import { startHealthWatch } from "./lib/health/watch.svelte";
  import Icon from "./lib/components/Icon.svelte";
  import ScanProgress from "./lib/components/ScanProgress.svelte";
  import CommandPalette from "./lib/components/CommandPalette.svelte";
  import DeleteDialog from "./lib/components/DeleteDialog.svelte";
  import Home from "./lib/views/Home.svelte";
  import Analyze from "./lib/views/Analyze.svelte";
  import OldLarge from "./lib/views/OldLarge.svelte";
  import Settings from "./lib/views/Settings.svelte";
  import Tidy from "./lib/views/Tidy.svelte";
  import Clean from "./lib/views/Clean.svelte";
  import CleanActivity from "./lib/views/CleanActivity.svelte";
  import Hardware from "./lib/views/Hardware.svelte";
  import DevCache from "./lib/views/DevCache.svelte";
  import Duplicates from "./lib/views/Duplicates.svelte";
  import Monitor from "./lib/views/Monitor.svelte";
  import { startMonitorSync } from "./lib/monitor/sync.svelte";
  import Health from "./lib/views/Health.svelte";
  import { scheduleReminder } from "./lib/devcache/state.svelte";
  import { openStartupDupes } from "./lib/dupes/state.svelte";
  import { currentOs, hasFeature, modKey, viewAvailable } from "./lib/platform";
  import MacAccessHint from "./lib/components/MacAccessHint.svelte";

  type NavItem = { id: View; icon: string; label: string };
  type NavGroup = { group: string; icon: string; label: string; items: NavItem[] };

  // Grouped by what the user wants to do; the cleanup history always stays last.
  const NAV: (NavItem | NavGroup)[] = [
    { id: "home", icon: "home", label: "nav-home" },
    { id: "analyze", icon: "analyze", label: "nav-analyze" },
    {
      group: "free",
      icon: "clean",
      label: "nav-group-free",
      items: [
        { id: "clean", icon: "clean", label: "nav-clean" },
        { id: "devcache", icon: "devcache", label: "nav-devcache" },
        { id: "duplicates", icon: "duplicates", label: "nav-duplicates" },
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
      ],
    },
  ];

  const isGroup = (entry: NavItem | NavGroup): entry is NavGroup => "group" in entry;

  // what this platform has (macOS: no Clean, Health, Hardware or Monitor)
  const os = $derived(currentOs(app.info?.os));
  const nav = $derived(
    NAV.flatMap((entry): (NavItem | NavGroup)[] => {
      if (!isGroup(entry)) return viewAvailable(os, entry.id) ? [entry] : [];
      const items = entry.items.filter((item) => viewAvailable(os, item.id));
      return items.length ? [{ ...entry, items }] : [];
    }),
  );
  $effect(() => {
    if (!viewAvailable(os, app.view)) app.view = "home";
  });

  // Groups the user opened or closed by hand; the group of the open view is always shown.
  let opened = $state<Record<string, boolean>>({});
  const groupOpen = (g: NavGroup) =>
    opened[g.group] ?? g.items.some((item) => item.id === app.view);
  const toggleGroup = (g: NavGroup) => (opened[g.group] = !groupOpen(g));
  $effect(() => {
    // jumping to a view (palette, Home buttons, shortcuts) reopens its group
    const view = app.view;
    for (const entry of NAV) {
      if (isGroup(entry) && entry.items.some((item) => item.id === view)) opened[entry.group] = true;
    }
  });

  let systemDark = $state(true);

  onMount(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    systemDark = mq.matches;
    const onChange = (e: MediaQueryListEvent) => (systemDark = e.matches);
    mq.addEventListener("change", onChange);
    void (async () => {
      await loadSettings();
      applyLocale();
      await api.appInfo().then((info) => (app.info = info)).catch(() => {});
      await listenScanEvents();
      const here = currentOs(app.info?.os);
      if (hasFeature(here, "sensors")) startThermalSync();
      if (hasFeature(here, "monitor")) startMonitorSync();
      if (hasFeature(here, "health")) startHealthWatch();
      scheduleReminder();
      await refreshVolumes();
      await openStartupPath();
      await openStartupDupes();
    })();
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === ",") {
        e.preventDefault();
        app.view = "settings";
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k" && !app.deleting) {
        e.preventDefault();
        app.palette = !app.palette;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      mq.removeEventListener("change", onChange);
      window.removeEventListener("keydown", onKey);
    };
  });

  // theme, locale and text size follow the settings live (no restart)
  $effect(() => {
    const { colors, dark } = resolvePalette(app.settings.theme, systemDark, app.info?.accent ?? null);
    applyPalette(colors, dark);
  });
  $effect(() => {
    document.documentElement.style.setProperty("--font-scale", String(app.settings.fontScale));
  });
  $effect(() => {
    void app.settings.locale;
    applyLocale();
  });
  $effect(() => {
    // persist any settings change once the stored ones are loaded
    JSON.stringify(app.settings);
    if (app.settingsLoaded) saveSettingsSoon();
  });
</script>

<div class="shell">
  <nav class="side" aria-label="Main">
    <div class="brand">
      <img src="/icon.png" alt="" width="34" height="34" />
      <div>
        <div class="name">NANO<span>FreeDisk</span></div>
        <div class="ver muted">{app.info?.version ?? ""}</div>
      </div>
    </div>
    <button class="find" onclick={() => (app.palette = true)} title={t("palette-title")}>
      <Icon name="search" size={16} />
      <span class="ellipsis">{t("palette-open")}</span>
      <kbd>{modKey(os)} K</kbd>
    </button>
    {#snippet navButton(item: NavItem, child: boolean)}
      <button
        class="nav"
        class:child
        class:active={item.id === app.view}
        data-view={item.id}
        title={t(item.label)}
        onclick={() => (app.view = item.id)}
        aria-current={item.id === app.view ? "page" : undefined}
      >
        <Icon name={item.icon} size={child ? 16 : 18} />
        <span class="ellipsis">{t(item.label)}</span>
      </button>
    {/snippet}
    <ul class="nav-list">
      {#each nav as entry (entry.label)}
        {#if isGroup(entry)}
          {@const open = groupOpen(entry)}
          {@const holdsActive = entry.items.some((item) => item.id === app.view)}
          <li class="group">
            <button
              class="group-head"
              class:holds-active={holdsActive && !open}
              aria-expanded={open}
              aria-controls={`nav-${entry.group}`}
              onclick={() => toggleGroup(entry)}
            >
              <Icon name={entry.icon} />
              <span class="ellipsis">{t(entry.label)}</span>
              <span class="caret" class:open><Icon name="chevron" size={14} /></span>
            </button>
            <ul id={`nav-${entry.group}`} class="children" hidden={!open}>
              {#each entry.items as item (item.id)}
                <li>{@render navButton(item, true)}</li>
              {/each}
            </ul>
          </li>
        {:else}
          <li>{@render navButton(entry, false)}</li>
        {/if}
      {/each}
    </ul>
    <div class="spacer"></div>
    <!-- the cleanup history always stays last, apart from the task groups -->
    {@render navButton({ id: "activity", icon: "undo", label: "nav-activity" }, false)}
    <div class="divider"></div>
    <div class="mode" role="radiogroup" aria-label={t("mode-label")}>
      {#each ["simple", "expert"] as const as m (m)}
        <button
          role="radio"
          aria-checked={app.settings.mode === m}
          class:on={app.settings.mode === m}
          data-effect
          title={`${t(`mode-${m}-hint`)} ${t("hint-mode")}`}
          onclick={() => (app.settings.mode = m)}
        >
          {t(`mode-${m}`)}
        </button>
      {/each}
    </div>
    <button
      class="nav"
      class:active={app.view === "settings"}
      onclick={() => (app.view = "settings")}
      title={`${modKey(os)}+,`}
    >
      <Icon name="settings" />
      <span>{t("nav-settings")}</span>
    </button>
  </nav>

  <main>
    <MacAccessHint />
    {#if app.view === "home"}
      <Home />
    {:else if app.view === "analyze"}
      <Analyze />
    {:else if app.view === "oldlarge"}
      <OldLarge />
    {:else if app.view === "tidy"}
      <Tidy />
    {:else if app.view === "clean"}
      <Clean />
    {:else if app.view === "activity"}
      <CleanActivity />
    {:else if app.view === "hardware"}
      <Hardware />
    {:else if app.view === "devcache"}
      <DevCache />
    {:else if app.view === "duplicates"}
      <Duplicates />
    {:else if app.view === "monitor"}
      <Monitor />
    {:else if app.view === "health"}
      <Health />
    {:else}
      <Settings />
    {/if}
  </main>
</div>

{#if app.scanning}
  <ScanProgress />
{/if}
{#if app.deleting}
  <DeleteDialog ids={app.deleting} />
{/if}
{#if app.palette}
  <CommandPalette />
{/if}

<div class="toasts" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast {toast.kind}">{toast.text}</div>
  {/each}
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 220px 1fr;
    height: 100%;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 14px 10px;
    background: var(--surface);
    border-right: 1px solid var(--border);
    min-height: 0;
  }
  .brand {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 4px 8px 16px;
  }
  .brand img {
    border-radius: 8px;
  }
  .name {
    font-weight: 800;
    letter-spacing: 0.2px;
    font-size: 1.08em;
  }
  .name span {
    color: var(--accent);
  }
  .ver {
    font-size: 0.78em;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    text-align: left;
  }
  .nav:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .nav.active {
    background: var(--accent-soft);
    color: var(--text);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .nav.child {
    padding: 6px 10px 6px 34px;
    font-size: 0.95em;
  }
  .nav-list {
    overflow-y: auto;
    min-height: 0;
  }
  .group {
    margin-top: 6px;
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    font-weight: 600;
    text-align: left;
  }
  .group-head:hover {
    background: var(--hover);
  }
  .group-head.holds-active {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .caret {
    display: flex;
    margin-left: auto;
    color: var(--text-muted);
    transition: transform 0.15s ease;
  }
  .caret.open {
    transform: rotate(90deg);
  }
  .children {
    list-style: none;
    margin: 2px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .children[hidden] {
    display: none;
  }
  .spacer {
    flex: 1;
  }
  .divider {
    height: 1px;
    margin: 6px 4px 8px;
    background: var(--border);
  }
  .find {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 10px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text-muted);
    text-align: left;
  }
  .find:hover {
    border-color: var(--accent);
    color: var(--text);
  }
  .find span {
    flex: 1;
  }
  kbd {
    font-family: inherit;
    font-size: 0.75em;
    padding: 0 4px;
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .mode {
    display: grid;
    grid-template-columns: 1fr 1fr;
    margin-bottom: 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .mode button {
    border: none;
    background: transparent;
    padding: 5px 4px;
    color: var(--text-muted);
    font-size: 0.86em;
  }
  .mode button.on {
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 600;
  }
  main {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 50;
    max-width: 440px;
  }
  .toast {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-left: 4px solid var(--accent);
    padding: 10px 14px;
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow);
  }
  .toast.ok {
    border-left-color: var(--risk-safe);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
</style>
