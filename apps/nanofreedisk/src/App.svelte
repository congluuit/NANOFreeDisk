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
  import Icon from "./lib/components/Icon.svelte";
  import ScanProgress from "./lib/components/ScanProgress.svelte";
  import CommandPalette from "./lib/components/CommandPalette.svelte";
  import DeleteDialog from "./lib/components/DeleteDialog.svelte";
  import Home from "./lib/views/Home.svelte";
  import Analyze from "./lib/views/Analyze.svelte";
  import OldLarge from "./lib/views/OldLarge.svelte";
  import Settings from "./lib/views/Settings.svelte";
  import Tidy from "./lib/views/Tidy.svelte";

  const NAV: { id: View | null; icon: string; label: string; phase?: string }[] = [
    { id: "home", icon: "home", label: "nav-home" },
    { id: "analyze", icon: "analyze", label: "nav-analyze" },
    { id: "oldlarge", icon: "drive", label: "nav-oldlarge" },
    { id: "tidy", icon: "folder", label: "nav-tidy" },
    { id: null, icon: "clean", label: "nav-clean", phase: "Phase 3" },
    { id: null, icon: "devcache", label: "nav-devcache", phase: "Phase 4" },
    { id: null, icon: "duplicates", label: "nav-duplicates", phase: "Phase 5" },
    { id: null, icon: "monitor", label: "nav-monitor", phase: "Phase 6" },
    { id: null, icon: "health", label: "nav-health", phase: "Phase 7" },
  ];

  let systemDark = $state(true);

  onMount(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    systemDark = mq.matches;
    const onChange = (e: MediaQueryListEvent) => (systemDark = e.matches);
    mq.addEventListener("change", onChange);
    void (async () => {
      await loadSettings();
      applyLocale();
      api.appInfo().then((info) => (app.info = info)).catch(() => {});
      await listenScanEvents();
      await refreshVolumes();
      await openStartupPath();
    })();
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === ",") {
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
      <kbd>Ctrl K</kbd>
    </button>
    <ul>
      {#each NAV as item (item.label)}
        <li>
          <button
            class="nav"
            class:active={item.id === app.view}
            disabled={!item.id}
            title={item.phase ? t("nav-soon", { phase: item.phase }) : t(item.label)}
            onclick={() => item.id && (app.view = item.id)}
            aria-current={item.id === app.view ? "page" : undefined}
          >
            <Icon name={item.icon} />
            <span class="ellipsis">{t(item.label)}</span>
            {#if item.phase}<small>{item.phase.replace("Phase ", "P")}</small>{/if}
          </button>
        </li>
      {/each}
    </ul>
    <div class="spacer"></div>
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
      title="Ctrl+,"
    >
      <Icon name="settings" />
      <span>{t("nav-settings")}</span>
    </button>
  </nav>

  <main>
    {#if app.view === "home"}
      <Home />
    {:else if app.view === "analyze"}
      <Analyze />
    {:else if app.view === "oldlarge"}
      <OldLarge />
    {:else if app.view === "tidy"}
      <Tidy />
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
  .nav:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .nav small {
    margin-left: auto;
    font-size: 0.72em;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 4px;
  }
  .spacer {
    flex: 1;
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
