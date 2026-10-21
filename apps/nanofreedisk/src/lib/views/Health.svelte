<script lang="ts">
  // Health view (docs/PHASE7_HEALTH.md §2.6): one card per physical drive with a
  // CrystalDiskInfo-style verdict, the reasons behind it, the NVMe log / SMART table
  // and the read-only volume checks. Nothing here changes a drive.
  import { api, errorText, inTauri, pickSaveFile } from "../api";
  import { download } from "../actions";
  import { formatCount, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, expert, fail, toast } from "../state.svelte";
  import { paletteVars, resolvePalette } from "../theme";
  import { healthApi } from "../health/api";
  import { buildHealthReport } from "../health/report";
  import type { Drive, HealthReport } from "../health/types";
  import { cliCommand, heatClass, hours, nvmeRows, reasonArgs, statusToken, volumeRows } from "../health/view";
  import CliHint from "../components/CliHint.svelte";
  import Icon from "../components/Icon.svelte";
  import StatCard from "../components/StatCard.svelte";

  const loc = $derived(intlLocale());
  let report = $state<HealthReport | null>(null);
  let loading = $state(false);
  let elevating = $state(false);
  let error = $state<string | null>(null);
  let selected = $state<number | null>(null);
  let serials = $state(false);

  const counts = $derived({
    good: report?.drives.filter((d) => d.status === "good").length ?? 0,
    attention: report?.drives.filter((d) => d.status === "caution" || d.status === "bad").length ?? 0,
  });
  const hottest = $derived(
    (report?.drives ?? []).filter((d) => d.temperature != null).sort((a, b) => (b.temperature ?? 0) - (a.temperature ?? 0))[0] ?? null,
  );
  const drive = $derived(report?.drives.find((d) => d.index === selected) ?? report?.drives[0] ?? null);
  // Simple mode shows the attributes the verdict and the cards use
  const KEY_ATTRS = new Set([0x05, 0x09, 0x0c, 0xc2, 0xbe, 0xc5, 0xc6, 0xc7, 0xe7, 0xe9, 0xb1, 0xa9, 0xad, 0xf1]);

  async function load(refresh: boolean) {
    loading = true;
    error = null;
    try {
      report = await healthApi.report(refresh);
    } catch (e) {
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void load(false);
  });

  async function elevate() {
    elevating = true;
    try {
      report = await healthApi.elevated();
      toast(t("health-elevated-done"), "ok");
    } catch (e) {
      const code = (e as { code?: string })?.code;
      if (code === "declined") toast(t("health-elevated-declined"));
      else fail(e);
    } finally {
      elevating = false;
    }
  }

  async function exportHtml() {
    if (!report) return;
    const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    const palette = resolvePalette(app.settings.theme, dark, app.info?.accent ?? null);
    const html = buildHealthReport(
      { report, vars: paletteVars(palette.colors, palette.dark), dark: palette.dark, version: app.info?.version ?? "", locale: loc, serials },
      t,
    );
    const name = "nanofreedisk-health.html";
    if (!inTauri) {
      download(name, html);
      return;
    }
    const path = await pickSaveFile(t("health-export-title"), name, "html");
    if (!path) return;
    try {
      await api.saveHtmlReport(path, html);
      toast(t("health-exported", { path }), "ok");
    } catch (e) {
      fail(e);
    }
  }

  async function exportJson() {
    const path = await pickSaveFile(t("health-export-title"), "nanofreedisk-health.json", "json");
    if (!path) return;
    try {
      await healthApi.exportJson(path, serials);
      toast(t("health-exported", { path }), "ok");
    } catch (e) {
      fail(e);
    }
  }

  function summary(d: Drive): string {
    const parts = [d.bus];
    if (d.size) parts.push(formatSize(d.size, loc));
    if (d.ssd != null) parts.push(t(d.ssd ? "health-ssd" : "health-hdd"));
    return parts.join(" · ");
  }

  const hex = (n: number) => n.toString(16).toUpperCase().padStart(2, "0");
</script>

<section class="page health-view">
  <header class="page-head">
    <div>
      <h1>{t("health-title-view")}</h1>
      <p>{t("health-subtitle")}</p>
    </div>
    <div class="page-actions">
      <button class="btn ghost icon" data-health-refresh onclick={() => load(true)} disabled={loading} title={t("health-refresh")} aria-label={t("health-refresh")}>
        <Icon name="refresh" />
      </button>
      {#if inTauri && report && !report.elevated}
        <button class="btn" data-effect data-health-elevate onclick={elevate} disabled={elevating} title={t("health-elevate-hint")}>
          <Icon name="shield" />{t("health-elevate")}
        </button>
      {/if}
      <button class="btn" data-effect data-health-html onclick={exportHtml} disabled={!report} title={t("health-export-html-hint")}>
        <Icon name="export" />{t("health-export-html")}
      </button>
      {#if expert()}
        <button class="btn" data-effect data-health-json onclick={exportJson} disabled={!report || !inTauri} title={t("health-export-json-hint")}>
          <Icon name="export" />{t("health-export-json")}
        </button>
        <label class="check" title={t("health-serials-hint")}>
          <input type="checkbox" bind:checked={serials} data-effect title={t("health-serials-hint")} />{t("health-serials")}
        </label>
      {/if}
    </div>
  </header>

  <div class="scroll">
    {#if error}
      <p class="err">{t("health-error", { message: error })}</p>
    {:else if !report}
      <p class="muted">{t("health-loading")}</p>
    {:else}
      <div class="stats">
        <StatCard label={t("stat-drives")} value={String(report.drives.length)} tone={counts.attention ? "caution" : report.drives.length ? "safe" : null} unit={counts.attention ? t("stat-drives-attention", { count: counts.attention }) : t("stat-drives-good")} />
        <StatCard label={t("stat-hottest")} value={hottest ? `${Math.round(hottest.temperature ?? 0)} °C` : null} unit={hottest?.model ?? ""} tone={hottest ? (hottest.heat === "critical" ? "protected" : hottest.heat === "warning" ? "caution" : null) : null} />
        <StatCard label={t("health-volumes")} value={String(report.volumes.length)} unit={t("stat-volumes-checked")} />
      </div>
      {#if !report.elevated}
        <p class="muted small note"><Icon name="lock" size={14} />{t("health-standard-user")}</p>
      {/if}
      {#if !report.drives.length}
        <p class="muted">{t("health-no-drives")}</p>
      {/if}
      <div class="drives">
        {#each report.drives as d (d.index)}
          <button
            class="card drive"
            aria-pressed={drive?.index === d.index}
            class:on={drive?.index === d.index}
            data-drive={d.index}
            data-status={d.status}
            onclick={() => (selected = d.index)}
            style={`--tone: var(${statusToken(d.status)})`}
          >
            <span class="top">
              <Icon name={d.bus === "USB" ? "usb" : "drive"} size={18} />
              <strong class="ellipsis" title={d.model}>{d.model}</strong>
            </span>
            <span class="badge">{t(`health-status-${d.status}`)}{d.lifeLeft != null ? ` · ${d.lifeLeft} %` : ""}</span>
            <span class="muted small">{summary(d)}</span>
            <span class="facts small">
              {#if d.temperature != null}<span class="temp {heatClass(d.heat)}"><Icon name="thermo" size={13} />{Math.round(d.temperature)} °C</span>{/if}
              {#if d.powerOnHours != null}<span><Icon name="clock" size={13} />{formatCount(d.powerOnHours, loc)} h</span>{/if}
              {#if d.volumes.length}<span>{d.volumes.map((v) => v.replace(/\\$/, "")).join(" ")}</span>{/if}
            </span>
          </button>
        {/each}
      </div>

      {#if drive}
        {@const d = drive}
        <article class="panel detail" data-detail={d.index}>
          <h2>
            <span class="dot" style={`background: var(${statusToken(d.status)})`}></span>
            {d.model}
            <span class="muted small">{t(`health-source-${d.source}`)}</span>
          </h2>
          {#if d.reasons.length}
            <ul class="reasons">
              {#each d.reasons as r, i (i)}
                <li><Icon name="alert" size={14} />{t(`health-reason-${r.key}`, reasonArgs(r, d))}</li>
              {/each}
            </ul>
          {:else if d.status === "good"}
            <p class="ok"><Icon name="check" size={16} />{t("health-no-problems")}</p>
          {/if}
          {#if d.note}
            <p class="muted small note"><Icon name="lock" size={14} />{t(`health-note-${d.note}`)}</p>
          {/if}
          <dl>
            {#if d.temperature != null}
              <dt class="muted">{t("health-temperature")}</dt>
              <dd>
                <span class="temp {heatClass(d.heat)}">{Math.round(d.temperature)} °C</span>
                {#if d.tempWarning != null}<span class="muted small">{t("health-temp-limits", { warning: Math.round(d.tempWarning), critical: Math.round(d.tempCritical ?? d.tempWarning) })}</span>{/if}
              </dd>
            {/if}
            {#if d.lifeLeft != null}<dt class="muted">{t("health-life")}</dt><dd>{d.lifeLeft} %</dd>{/if}
            {#if d.powerOnHours != null}<dt class="muted">{t("health-power-on")}</dt><dd>{hours(d.powerOnHours, t, loc)}</dd>{/if}
            {#if d.powerCycles != null}<dt class="muted">{t("health-power-cycles")}</dt><dd>{formatCount(d.powerCycles, loc)}</dd>{/if}
            {#if d.bytesWritten != null}<dt class="muted">{t("health-written")}</dt><dd>{formatSize(d.bytesWritten, loc)}</dd>{/if}
            {#if d.bytesRead != null}<dt class="muted">{t("health-read")}</dt><dd>{formatSize(d.bytesRead, loc)}</dd>{/if}
            {#if d.firmware}<dt class="muted">{t("health-firmware")}</dt><dd>{d.firmware}</dd>{/if}
            {#if d.serial}<dt class="muted">{t("health-serial")}</dt><dd class="mono">{d.serial}</dd>{/if}
            {#if d.trim != null}<dt class="muted">{t("health-trim-support")}</dt><dd>{t(d.trim ? "health-yes" : "health-no")}</dd>{/if}
            {#each nvmeRows(d, t, loc, expert()) as [k, v] (k)}
              <dt class="muted">{k}</dt><dd>{v}</dd>
            {/each}
          </dl>
          {#if d.ata?.attributes.length}
            <div class="tablewrap">
              <table data-smart>
                <thead>
                  <tr>
                    <th>ID</th><th>{t("health-attr-name")}</th>
                    <th class="num">{t("health-attr-current")}</th><th class="num">{t("health-attr-worst")}</th>
                    <th class="num">{t("health-attr-threshold")}</th><th class="num">{t("health-attr-raw")}</th>
                  </tr>
                </thead>
                <tbody>
                  {#each d.ata.attributes.filter((a) => expert() || KEY_ATTRS.has(a.id)) as a (a.id)}
                    {@const failing = a.threshold != null && a.threshold !== 0 && a.current !== 0 && a.current < a.threshold}
                    <tr class:bad={failing}>
                      <td class="mono">{hex(a.id)}</td>
                      <td>{a.name ?? t("health-attr-vendor")}</td>
                      <td class="num">{a.current}</td>
                      <td class="num">{a.worst}</td>
                      <td class="num">{a.threshold ?? "—"}</td>
                      <td class="num mono">{expert() ? `${formatCount(a.raw, loc)} (0x${a.raw.toString(16).toUpperCase()})` : formatCount(a.raw, loc)}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            {#if !d.ata.thresholds}<p class="muted small">{t("health-no-thresholds")}</p>{/if}
          {/if}
        </article>
      {/if}

      <h2 class="section">{t("health-volumes")}</h2>
      <div class="vols">
        {#each report.volumes as v (v.root)}
          {@const d = report.drives.find((x) => x.index === v.disk)}
          <article class="panel vol" data-volume={v.root}>
            <h3>{v.root.replace(/\\$/, "")} {#if v.label}<span class="muted">{v.label}</span>{/if}</h3>
            <dl>
              {#each volumeRows(v, d, t, loc) as [k, val, bad] (k)}
                <dt class="muted">{k}</dt><dd class:bad>{val}</dd>
              {/each}
            </dl>
          </article>
        {/each}
      </div>
      <CliHint command={cliCommand(report.elevated)} />
    {/if}
  </div>
</section>

<style>
  .health-view {
    display: flex;
    flex-direction: column;
    padding-bottom: 0;
    min-height: 0;
    overflow: hidden;
  }
  .health-view > .page-head {
    margin-bottom: 14px;
  }
  .check {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    font-size: 0.9em;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-bottom: 24px;
  }
  .err {
    color: var(--danger);
  }
  .note {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 0 0 12px;
  }
  .drives {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  .drive {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 14px 16px;
    text-align: left;
    color: var(--text);
    font: inherit;
    cursor: pointer;
    min-width: 0;
    transition: border-color 0.12s;
  }
  .drive:hover:not(.on) {
    border-color: var(--text-muted);
  }
  .drive.on {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .top {
    display: flex;
    gap: 8px;
    align-items: center;
    width: 100%;
    min-width: 0;
  }
  .badge {
    background: color-mix(in srgb, var(--tone) 16%, var(--surface));
    color: color-mix(in srgb, var(--tone) 60%, var(--text));
    border-radius: 999px;
    padding: 1px 10px;
    font-weight: 600;
    font-size: 0.82em;
  }
  .facts {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    color: var(--text-muted);
  }
  .facts span {
    display: inline-flex;
    gap: 4px;
    align-items: center;
  }
  .temp {
    font-weight: 600;
  }
  .temp.ok {
    color: var(--temp-ok);
  }
  .temp.warm {
    color: var(--temp-warm);
  }
  .temp.hot {
    color: var(--temp-hot);
  }
  .detail {
    margin-top: 14px;
  }
  h2 {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    margin: 0 0 10px;
    font-size: 1.05em;
  }
  h2.section {
    margin: 20px 0 10px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 1em;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .reasons {
    margin: 0 0 10px;
    padding: 0;
    list-style: none;
  }
  .reasons li {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--risk-caution-ink);
  }
  .ok {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--risk-safe-ink);
    margin: 0 0 10px;
  }
  dl {
    display: grid;
    grid-template-columns: minmax(170px, auto) 1fr;
    gap: 3px 14px;
    margin: 0 0 10px;
    font-size: 0.92em;
  }
  dt,
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  dd .small {
    margin-left: 8px;
  }
  dd.bad,
  tr.bad td {
    color: var(--risk-protected);
    font-weight: 600;
  }
  .tablewrap {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9em;
  }
  th {
    text-align: left;
    font-weight: normal;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border);
    padding: 4px 8px;
  }
  td {
    padding: 3px 8px;
    border-bottom: 1px solid var(--border);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .vols {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 12px;
    margin-bottom: 12px;
  }

</style>
