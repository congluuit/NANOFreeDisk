<script lang="ts">
  // Settings, "Automatic cleaning" (docs/SCHEDULE.md): the low space alert, the
  // automatic clean and its categories, starting with Windows in the tray, the
  // last automatic clean and "Run now" with a preview first. Everything is off
  // until the user turns it on.
  import { inTauri } from "../api";
  import Icon from "../components/Icon.svelte";
  import { formatDateTime, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { currentOs } from "../platform";
  import { app, fail, toast } from "../state.svelte";
  import { scheduleApi } from "./api";
  import { categoryLabel, noteTexts, sanitize, setCategory } from "./model";
  import type { Frequency, ScheduleConfig, SchedulePreview, ScheduleState } from "./types";
  import { scheduleWatch, startScheduleWatch } from "./watch.svelte";

  const FREQUENCIES: Frequency[] = ["off", "weekly", "monthly"];
  const os = $derived(currentOs(app.info?.os));

  let s = $state<ScheduleState | null>(null);
  let preview = $state<SchedulePreview | null>(null);
  let busy = $state<"" | "save" | "preview" | "run" | "autostart">("");

  // normally started with the app; harmless when it already runs
  startScheduleWatch();
  $effect(() => {
    void scheduleWatch.version;
    scheduleApi
      .get()
      .then((x) => (s = x))
      .catch(() => (s = null));
  });

  const codeOf = (e: unknown): string => (e && typeof e === "object" && "code" in e ? String((e as { code: unknown }).code) : "");

  async function save(patch: Partial<ScheduleConfig>) {
    if (!s) return;
    busy = "save";
    try {
      s = await scheduleApi.set(sanitize({ ...s.config, ...patch }), noteTexts(t));
      preview = null;
    } catch (e) {
      fail(e);
    } finally {
      busy = "";
    }
  }

  async function setAutostart(on: boolean) {
    if (!s) return;
    busy = "autostart";
    try {
      s.autostart = await scheduleApi.autostartSet(on);
      toast(t(on ? "schedule-autostart-on" : "schedule-autostart-off"), "ok");
    } catch (e) {
      fail(e);
    } finally {
      busy = "";
    }
  }

  async function showPreview() {
    busy = "preview";
    try {
      preview = await scheduleApi.preview();
    } catch (e) {
      fail(e);
    } finally {
      busy = "";
    }
  }

  async function runNow() {
    busy = "run";
    try {
      // the result toast comes with the "schedule-cleaned" event
      await scheduleApi.runNow();
      preview = null;
      s = await scheduleApi.get();
    } catch (e) {
      preview = null;
      const code = codeOf(e);
      if (code === "no-preview") toast(t("schedule-preview-expired"), "error");
      else if (code === "nothing-to-clean") toast(t("schedule-preview-empty"), "ok");
      else fail(e);
    } finally {
      busy = "";
    }
  }

  const numberOf = (e: Event): number => Number((e.currentTarget as HTMLInputElement).value);
</script>

<div class="block panel" data-settings="schedule">
  <h2>{t("schedule-title")}</h2>
  <p class="muted small">{t("schedule-sub")}</p>
  {#if s}
    <h3>{t("schedule-low-heading")}</h3>
    <label class="toggle" title={t("schedule-low-hint")}>
      <input type="checkbox" checked={s.config.lowSpace} disabled={busy !== ""} onchange={(e) => save({ lowSpace: (e.currentTarget as HTMLInputElement).checked })} data-effect />
      <span>{t("schedule-low-toggle")}</span>
    </label>
    <div class="row">
      <label
        >{t("schedule-low-percent")}
        <input type="number" min="1" max="50" step="1" value={s.config.lowPercent} disabled={!s.config.lowSpace} onchange={(e) => save({ lowPercent: numberOf(e) })} data-effect />
      </label>
      <label
        >{t("schedule-low-gb")}
        <input type="number" min="1" max="10000" step="1" value={s.config.lowGb} disabled={!s.config.lowSpace} onchange={(e) => save({ lowGb: numberOf(e) })} data-effect />
      </label>
    </div>
    <p class="muted small">{t("schedule-low-hint")}</p>

    {#if s.supported}
      <h3>{t("schedule-clean")}</h3>
      <div class="row">
        <label
          >{t("schedule-clean-label")}
          <select value={s.config.frequency} disabled={busy !== ""} onchange={(e) => save({ frequency: (e.currentTarget as HTMLSelectElement).value as Frequency })} data-effect>
            {#each FREQUENCIES as f (f)}
              <option value={f}>{t(`schedule-freq-${f}`)}</option>
            {/each}
          </select>
        </label>
      </div>
      <label class="toggle">
        <input type="checkbox" checked={s.config.idleOnly} disabled={busy !== "" || s.config.frequency === "off"} onchange={(e) => save({ idleOnly: (e.currentTarget as HTMLInputElement).checked })} data-effect />
        <span>{t("schedule-idle")}</span>
      </label>
      <p class="muted small">{t("schedule-what")}</p>
      {#if s.categories.length}
        <fieldset class="cats" disabled={busy !== ""}>
          <legend class="muted small">{t("schedule-categories")}</legend>
          {#each s.categories as c (c)}
            <label class="toggle">
              <input type="checkbox" checked={!s.config.offCategories.includes(c)} onchange={(e) => save({ offCategories: setCategory(s!.config.offCategories, c, (e.currentTarget as HTMLInputElement).checked) })} />
              <span>{categoryLabel(c, t)}</span>
            </label>
          {/each}
        </fieldset>
      {/if}
      {#if s.config.frequency !== "off"}
        <p class="note" class:warn={!s.autostart.on}>
          <Icon name="info" size={16} />
          {t("schedule-needs-running")}
        </p>
      {/if}
    {:else}
      <p class="muted small">{t("schedule-unsupported")}</p>
    {/if}

    {#if s.autostart.supported}
      <h3>{t("schedule-autostart-heading")}</h3>
      <label class="toggle">
        <input type="checkbox" checked={s.autostart.on} disabled={busy !== "" || !inTauri} onchange={(e) => setAutostart((e.currentTarget as HTMLInputElement).checked)} data-effect />
        <span>{t(os === "macos" ? "schedule-autostart-mac" : "schedule-autostart")}</span>
      </label>
      <p class="muted small">{t("schedule-autostart-hint")}</p>
      {#if s.autostart.blocked}
        <p class="note warn"><Icon name="alert" size={16} />{t("schedule-autostart-blocked")}</p>
      {/if}
    {/if}

    {#if s.supported}
      <div class="row status">
        <span class="lines">
          <span>
            {s.last
              ? t("schedule-last", { date: formatDateTime(s.last.at, intlLocale()), size: formatSize(s.last.freed, intlLocale()) })
              : t("schedule-last-none")}
          </span>
          {#if s.next}
            <span class="muted small">{t("schedule-next", { date: formatDateTime(s.next, intlLocale()) })}</span>
          {/if}
        </span>
        <span class="grow"></span>
        <button class="btn ghost" onclick={() => (app.view = "activity")} data-effect><Icon name="undo" size={16} />{t("schedule-history")}</button>
        <button class="btn" onclick={showPreview} disabled={busy !== ""} data-effect>
          <Icon name="clean" size={16} />{busy === "preview" ? t("schedule-busy") : t("schedule-run-now")}
        </button>
      </div>

      {#if preview}
        <div class="preview">
          <h3>{t("schedule-preview-title")}</h3>
          {#if preview.items === 0}
            <p class="muted">{t("schedule-preview-empty")}</p>
          {:else}
            <ul>
              {#each preview.rules as r (r.ruleId)}
                <li>
                  <span class="app">{r.app}</span>
                  <span class="muted small">{categoryLabel(r.category, t)}</span>
                  <span class="grow"></span>
                  <span class="num">{formatSize(r.bytes, intlLocale())}</span>
                </li>
              {/each}
            </ul>
            <p><strong>{t("schedule-preview-total", { count: preview.items, size: formatSize(preview.bytes, intlLocale()) })}</strong></p>
            <p class="muted small">{t("schedule-preview-where")}</p>
          {/if}
          <div class="row">
            <span class="grow"></span>
            <button class="btn ghost" onclick={() => (preview = null)} disabled={busy === "run"}>{t("schedule-preview-cancel")}</button>
            {#if preview.items > 0}
              <button class="btn primary" onclick={runNow} disabled={busy !== ""} data-effect>
                <Icon name="clean" size={16} />{busy === "run" ? t("schedule-busy") : t("schedule-preview-go")}
              </button>
            {/if}
          </div>
        </div>
      {/if}
    {/if}
  {:else}
    <p class="muted small">{t("schedule-loading")}</p>
  {/if}
</div>

<style>
  .block {
    padding: 16px 18px;
    flex-shrink: 0;
  }
  h2 {
    margin: 0 0 12px;
    font-size: 1.06em;
  }
  h3 {
    margin: 16px 0 8px;
    font-size: 0.95em;
  }
  .toggle {
    display: inline-flex;
    gap: 8px;
    align-items: center;
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
  .row input[type="number"] {
    width: 90px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
  }
  .grow {
    flex: 1;
  }
  .cats {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 6px 14px;
    margin: 8px 0 0;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .note {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 10px 0 0;
    font-size: 0.9em;
    color: var(--text-muted);
  }
  .note.warn {
    color: var(--risk-caution-ink);
  }
  .status {
    align-items: center;
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }
  .lines {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .preview {
    margin-top: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }
  .preview h3 {
    margin-top: 0;
  }
  .preview ul {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
    max-height: 240px;
    overflow: auto;
  }
  .preview li {
    display: flex;
    gap: 8px;
    align-items: baseline;
    padding: 3px 0;
    border-bottom: 1px solid var(--border);
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
</style>
