<script lang="ts">
  import { api, inTauri, pickOpenFile, pickSaveFile } from "../api";
  import type { HistoryInfo, UpdateInfo } from "../types";
  import { LOCALES, t } from "../i18n.svelte";
  import { DEFAULT_SETTINGS, app, fail, toast } from "../state.svelte";
  import {
    EDITABLE_TOKENS,
    PRESETS,
    contrastIssues,
    exportToml,
    importToml,
    resolvePalette,
    type PresetId,
    type ThemeMode,
    type Token,
  } from "../theme";
  import Icon from "../components/Icon.svelte";

  const MODES: { id: ThemeMode; icon: string }[] = [
    { id: "light", icon: "sun" },
    { id: "dark", icon: "disc" },
    { id: "system", icon: "settings" },
    { id: "custom", icon: "treemap" },
  ];

  const theme = $derived(app.settings.theme);
  const palette = $derived(
    resolvePalette(theme, window.matchMedia("(prefers-color-scheme: dark)").matches, app.info?.accent ?? null).colors,
  );
  const issues = $derived(theme.mode === "custom" ? contrastIssues(palette) : []);

  function setMode(mode: ThemeMode) {
    app.settings.theme.mode = mode;
  }

  function setColor(token: Token, value: string) {
    app.settings.theme.overrides = { ...app.settings.theme.overrides, [token]: value };
  }

  async function importTheme() {
    const path = await pickOpenFile(t("settings-theme-import"), "toml");
    if (!path) return;
    try {
      const parsed = importToml(await api.readTextFile(path));
      if (!parsed) {
        toast(t("settings-theme-invalid"), "error");
        return;
      }
      app.settings.theme = { mode: "custom", preset: parsed.preset, overrides: parsed.overrides };
      toast(t("settings-theme-imported"), "ok");
    } catch (e) {
      fail(e);
    }
  }

  let newVendor = $state("");
  function setThreshold(vendor: string, gb: number) {
    app.settings.devThresholds = { ...app.settings.devThresholds, [vendor]: Math.max(0, Number.isFinite(gb) ? gb : 0) };
  }
  function removeThreshold(vendor: string) {
    const rest = Object.fromEntries(Object.entries(app.settings.devThresholds).filter(([v]) => v !== vendor));
    // the backend has defaults for these: 0 turns their reminder off
    app.settings.devThresholds = vendor in DEFAULT_SETTINGS.devThresholds ? { ...rest, [vendor]: 0 } : rest;
  }
  function addThreshold() {
    const v = newVendor.trim();
    if (!v) return;
    setThreshold(v, 10);
    newVendor = "";
  }

  let history = $state<HistoryInfo | null>(null);
  let shellMenu = $state<boolean | null>(null);
  $effect(() => {
    api.shellMenuStatus().then((s) => (shellMenu = s)).catch(() => (shellMenu = null));
  });

  async function setShellMenu(enabled: boolean) {
    try {
      shellMenu = await api.shellMenuSet(enabled, t("shell-menu-caption"), t("shell-menu-dupes-caption"));
      toast(t(enabled ? "settings-shell-on" : "settings-shell-off"), "ok");
    } catch (e) {
      fail(e);
    }
  }
  $effect(() => {
    void app.version;
    api.historyInfo().then((h) => (history = h)).catch(() => (history = null));
  });

  async function clearHistory() {
    const question = t("settings-history-clear-confirm");
    let ok: boolean;
    if (inTauri) {
      const { ask } = await import("@tauri-apps/plugin-dialog");
      ok = await ask(question, { title: t("settings-history"), kind: "warning" });
    } else {
      ok = window.confirm(question);
    }
    if (!ok) return;
    try {
      await api.historyClear();
      app.snapshot = null;
      history = await api.historyInfo();
      toast(t("settings-history-cleared"), "ok");
    } catch (e) {
      fail(e);
    }
  }

  let update = $state<UpdateInfo | null>(null);
  let updateBusy = $state(false);
  async function checkUpdate() {
    updateBusy = true;
    try {
      update = await api.updateCheck();
      if (!update.available) toast(t("update-latest", { version: update.current }), "ok");
    } catch (e) {
      fail(e);
    } finally {
      updateBusy = false;
    }
  }
  async function installUpdate() {
    updateBusy = true;
    try {
      await api.updateInstall();
    } catch (e) {
      fail(e);
      updateBusy = false;
    }
  }

  async function exportTheme() {
    const path = await pickSaveFile(t("settings-theme-export"), "theme.toml", "toml");
    if (!path) return;
    try {
      await api.writeTextFile(path, exportToml("Custom", theme.preset, palette));
    } catch (e) {
      fail(e);
    }
  }
</script>

<section class="page">
  <h1>{t("settings-title")}</h1>

  <div class="block card">
    <h2>{t("settings-language")}</h2>
    <select bind:value={app.settings.locale} aria-label={t("settings-language")} data-effect title={t("hint-language")}>
      <option value="system">{t("settings-language-system")}</option>
      {#each LOCALES as l (l.id)}
        <option value={l.id}>{l.label}</option>
      {/each}
    </select>
  </div>

  <div class="block card">
    <h2>{t("settings-theme")}</h2>
    <div class="modes" role="radiogroup" aria-label={t("settings-theme")}>
      {#each MODES as m (m.id)}
        <button role="radio" aria-checked={theme.mode === m.id} class="mode" class:on={theme.mode === m.id} onclick={() => setMode(m.id)} data-effect title={t("hint-theme")}>
          <Icon name={m.icon} size={20} />
          {t(`settings-theme-${m.id}`)}
        </button>
      {/each}
    </div>
    {#if theme.mode === "system"}
      <p class="muted small">{t("settings-theme-system-hint")}</p>
    {/if}

    {#if theme.mode === "custom"}
      <div class="row">
        <label>
          <span class="muted small">{t("settings-preset")}</span>
          <select
            value={theme.preset}
            onchange={(e) => {
              app.settings.theme.preset = (e.currentTarget as HTMLSelectElement).value as PresetId;
              app.settings.theme.overrides = {};
            }}
          >
            {#each Object.entries(PRESETS) as [id, p] (id)}
              <option value={id}>{p.label}</option>
            {/each}
          </select>
        </label>
        <span class="grow"></span>
        <button class="btn" onclick={importTheme} disabled={!inTauri} data-effect title={t("hint-theme-import")}><Icon name="folder" size={16} />{t("settings-theme-import")}</button>
        <button class="btn" onclick={exportTheme} disabled={!inTauri} data-effect title={t("hint-export")}><Icon name="export" size={16} />{t("settings-theme-export")}</button>
        <button class="btn ghost" onclick={() => (app.settings.theme.overrides = {})} data-effect title={t("hint-theme-reset")}>{t("settings-theme-reset")}</button>
      </div>

      {#each issues as i (i.fg + i.bg)}
        <p class="warn">
          <Icon name="alert" size={16} />
          {t("settings-contrast-warning", { pair: `${t(`token-${i.fg}`)} / ${t(`token-${i.bg}`)}`, ratio: i.ratio.toFixed(1) })}
        </p>
      {/each}

      <h3>{t("settings-custom-colors")}</h3>
      <div class="colors">
        {#each EDITABLE_TOKENS as k (k)}
          <label class="color">
            <input type="color" value={palette[k]} oninput={(e) => setColor(k, (e.currentTarget as HTMLInputElement).value)} />
            <span>{t(`token-${k}`)}</span>
            <code class="muted">{palette[k]}</code>
          </label>
        {/each}
      </div>
    {:else}
      <div class="presets">
        {#each Object.entries(PRESETS) as [id, p] (id)}
          <button
            class="preset"
            data-effect
            title={`${p.label} — ${t("hint-theme-preset")}`}
            onclick={() => (app.settings.theme = { mode: "custom", preset: id as PresetId, overrides: {} })}
          >
            <span class="sample" style="background: {p.colors.bg}; border-color: {p.colors.border}">
              <i style="background: {p.colors.accent}"></i>
              <i style="background: {p.colors['tm-1']}"></i>
              <i style="background: {p.colors['tm-6']}"></i>
              <i style="background: {p.colors['tm-4']}"></i>
            </span>
            <span class="small">{p.label}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <div class="block card">
    <h2>{t("mode-label")}</h2>
    <div class="modes two" role="radiogroup" aria-label={t("mode-label")}>
      {#each ["simple", "expert"] as const as m (m)}
        <button role="radio" aria-checked={app.settings.mode === m} class="mode" class:on={app.settings.mode === m} onclick={() => (app.settings.mode = m)} data-effect title={t("hint-mode")}>
          <Icon name={m === "simple" ? "home" : "devcache"} size={20} />
          <span class="mtext">
            <strong>{t(`mode-${m}`)}</strong>
            <span class="muted small">{t(`mode-${m}-hint`)}</span>
          </span>
        </button>
      {/each}
    </div>
  </div>

  <div class="block card">
    <h2>{t("settings-shell")}</h2>
    <label class="toggle" title={t("hint-shell")}>
      <input
        type="checkbox"
        checked={shellMenu ?? false}
        disabled={shellMenu == null || !inTauri}
        onchange={(e) => setShellMenu((e.currentTarget as HTMLInputElement).checked)}
        data-effect
        title={t("hint-shell")}
      />
      <span>{t("settings-shell-toggle")}</span>
    </label>
    <p class="muted small">{t("settings-shell-hint")}</p>
  </div>

  <div class="block card">
    <h2>{t("settings-history")}</h2>
    <label class="toggle" title={t("hint-history-toggle")}>
      <input type="checkbox" bind:checked={app.settings.history} data-effect title={t("hint-history-toggle")} />
      <span>{t("settings-history-keep")}</span>
    </label>
    <p class="muted small">{t("settings-history-hint")}</p>
    <div class="row">
      {#if history}
        <span class="muted small">{t("settings-history-count", { count: history.snapshots })}</span>
        {#if app.settings.mode === "expert"}<code class="muted small ellipsis" title={history.path}>{history.path}</code>{/if}
      {/if}
      <span class="grow"></span>
      <button class="btn" onclick={clearHistory} disabled={!history?.snapshots} data-effect title={t("hint-history-clear")}>
        <Icon name="trash" size={16} />{t("settings-history-clear")}
      </button>
    </div>
  </div>

  <div class="block card" data-settings="health">
    <h2>{t("settings-health")}</h2>
    <label class="toggle" title={t("hint-health-alerts")}>
      <input type="checkbox" bind:checked={app.settings.healthAlerts} data-effect title={t("hint-health-alerts")} />
      <span>{t("settings-health-alerts")}</span>
    </label>
    <p class="muted small">{t("settings-health-alerts-note")}</p>
  </div>

  <div class="block card" data-settings="devcache">
    <h2>{t("settings-devcache")}</h2>
    <label class="toggle" title={t("hint-dev-alerts")}>
      <input type="checkbox" bind:checked={app.settings.devAlerts} data-effect title={t("hint-dev-alerts")} />
      <span>{t("settings-dev-alerts")}</span>
    </label>
    <p class="muted small">{t("settings-dev-thresholds-hint")}</p>
    <ul class="thresholds">
      {#each Object.entries(app.settings.devThresholds) as [vendor, gb] (vendor)}
        <li class="row">
          <span class="grow">{vendor}</span>
          <input
            type="number"
            min="0"
            step="1"
            value={gb}
            aria-label={t("settings-dev-threshold", { vendor })}
            onchange={(e) => setThreshold(vendor, Number((e.currentTarget as HTMLInputElement).value))}
            data-effect
            title={t("hint-dev-threshold")}
          />
          <span class="muted small">GB</span>
          <button class="btn ghost icon" onclick={() => removeThreshold(vendor)} aria-label={t("settings-dev-remove")} data-effect title={t("settings-dev-remove")}><Icon name="x" size={14} /></button>
        </li>
      {/each}
    </ul>
    <div class="row">
      <input class="vendor" bind:value={newVendor} placeholder={t("settings-dev-vendor")} aria-label={t("settings-dev-vendor")} />
      <button class="btn" disabled={!newVendor.trim()} onclick={addThreshold} data-effect title={t("hint-dev-threshold")}>{t("settings-dev-add")}</button>
    </div>
    <div class="row">
      <label>{t("settings-dev-keep-recent")}
        <input type="number" min="0" max="365" bind:value={app.settings.keepRecentDays} data-effect title={t("hint-dev-keep-recent")} />
      </label>
      <label>{t("settings-dev-stale")}
        <input type="number" min="0" max="3650" bind:value={app.settings.staleDays} data-effect title={t("hint-dev-stale")} />
      </label>
    </div>
  </div>

  <div class="block card">
    <h2>{t("settings-font-scale")}</h2>
    <div class="row">
      <input
        type="range"
        min="0.85"
        max="1.4"
        step="0.05"
        bind:value={app.settings.fontScale}
        aria-label={t("settings-font-scale")}
        data-effect
        title={t("hint-font")}
      />
      <span class="num">{Math.round(app.settings.fontScale * 100)}%</span>
    </div>
  </div>

  <div class="block card" data-settings="update">
    <h2>{t("update-title")}</h2>
    <p class="muted small">{t("update-hint")}</p>
    <div class="row">
      {#if update?.available}
        <span>{t("update-available", { version: update.available })}</span>
      {:else if update}
        <span class="muted small">{t("update-latest", { version: update.current })}</span>
      {/if}
      <span class="grow"></span>
      <button class="btn" onclick={checkUpdate} disabled={updateBusy || !inTauri} data-effect title={t("hint-update-check")}>{t("update-check")}</button>
      {#if update?.available && update.kind === "installer"}
        <button class="btn primary" onclick={installUpdate} disabled={updateBusy} data-effect title={t("hint-update-install")}>{t("update-install")}</button>
      {:else if update?.available}
        <button class="btn primary" onclick={() => api.updateOpenReleases().catch(fail)} data-effect title={t("hint-update-download")}>{t("update-download")}</button>
      {/if}
    </div>
    {#if update?.available && update.kind !== "installer"}
      <p class="muted small">{t(update.kind === "portable" ? "update-portable" : "update-setup")}</p>
    {/if}
    {#if update?.notes}<pre class="notes small">{update.notes}</pre>{/if}
  </div>

  <div class="block card about">
    <img src="/icon.png" alt="" width="56" height="56" />
    <div>
      <h2>{t("settings-about")}</h2>
      <p>{t("settings-about-text", { version: app.info?.version ?? "" })}</p>
      <p class="muted small">{t("settings-author")} · nanodev.io.vn · github.com/congluuit/NANOFreeDisk</p>
    </div>
  </div>
</section>

<style>
  .notes {
    max-height: 160px;
    overflow: auto;
    white-space: pre-wrap;
    margin: 8px 0 0;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--surface-2, rgba(127, 127, 127, 0.08));
  }
  .page {
    height: 100%;
    overflow: auto;
    padding: 28px 32px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 980px;
  }
  h1 {
    margin: 0 0 6px;
    font-size: 1.6em;
  }
  h2 {
    margin: 0 0 10px;
    font-size: 1.05em;
  }
  h3 {
    margin: 16px 0 8px;
    font-size: 0.95em;
  }
  .block {
    padding: 16px 18px;
  }
  .small {
    font-size: 0.86em;
  }
  .modes {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .mode {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
  }
  .toggle {
    display: inline-flex;
    gap: 8px;
    align-items: center;
  }
  .modes.two {
    grid-template-columns: 1fr 1fr;
  }
  .mtext {
    display: flex;
    flex-direction: column;
    text-align: left;
  }
  .mode.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .row {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    margin-top: 12px;
    flex-wrap: wrap;
  }
  .row label {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .grow {
    flex: 1;
  }
  .warn {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--risk-caution);
    margin: 8px 0 0;
    font-size: 0.9em;
  }
  .colors {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 6px 14px;
  }
  .color {
    display: grid;
    grid-template-columns: 34px 1fr auto;
    gap: 8px;
    align-items: center;
  }
  .color input {
    width: 34px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: none;
  }
  .presets {
    display: flex;
    gap: 12px;
    margin-top: 14px;
    flex-wrap: wrap;
  }
  .preset {
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: center;
    border: none;
    background: none;
    padding: 0;
  }
  .sample {
    display: flex;
    gap: 4px;
    padding: 10px;
    border-radius: 8px;
    border: 1px solid;
  }
  .sample i {
    width: 14px;
    height: 24px;
    border-radius: 3px;
  }
  .preset:hover .sample {
    outline: 2px solid var(--accent);
  }
  .thresholds {
    list-style: none;
    margin: 6px 0;
    padding: 0;
    max-width: 420px;
  }
  .thresholds input,
  .row input[type="number"],
  .vendor {
    width: 90px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text);
  }
  .vendor {
    width: 220px;
  }
  .about {
    display: flex;
    gap: 16px;
    align-items: center;
  }
  .about img {
    border-radius: 12px;
  }
  .about p {
    margin: 2px 0;
  }
  input[type="range"] {
    width: 260px;
    accent-color: var(--accent);
  }
</style>
