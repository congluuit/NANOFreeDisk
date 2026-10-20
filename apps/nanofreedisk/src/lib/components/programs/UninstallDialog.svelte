<script lang="ts">
  // The uninstall confirmation: which programs, what runs (the vendor's own
  // uninstaller, in its own window), what deserves care, and what happens after
  // (the leftovers are looked for and shown — nothing is removed without a tick).
  import { formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { expert } from "../../state.svelte";
  import { pg, programOf, runUninstalls } from "../../programs/state.svelte";
  import type { Program } from "../../programs/types";
  import Icon from "../Icon.svelte";

  let { ids }: { ids: string[] } = $props();
  let dialog = $state<HTMLDivElement | null>(null);
  let ack = $state(false);
  const loc = $derived(intlLocale());
  const list = $derived(ids.map(programOf).filter((p): p is Program => !!p && !p.blocked));
  const cautions = $derived(list.filter((p) => p.caution));
  const machine = $derived(list.some((p) => p.scope === "machine"));
  const total = $derived(list.reduce((s, p) => s + p.size, 0));

  $effect(() => {
    dialog?.querySelector<HTMLElement>(".cancel")?.focus();
  });

  function close() {
    pg.confirm = null;
  }
  function go() {
    if (cautions.length && !ack) return;
    void runUninstalls(list.map((p) => p.id));
  }
  const command = (p: Program) =>
    p.broken ? t("pg-confirm-broken") : p.source === "store" ? `Remove-AppxPackage ${p.package?.fullName ?? ""}` : pg.quiet && p.quietUninstall ? p.quietUninstall : p.uninstall?.raw || p.uninstall?.exe || "";
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={(e) => e.key === "Escape" && close()}>
  <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="pg-confirm-title" data-pg-confirm bind:this={dialog}>
    <h2 id="pg-confirm-title"><Icon name="trash" size={20} />{t("pg-confirm-title", { count: list.length })}</h2>
    <p>{t("pg-confirm-body", { size: formatSize(total, loc) })}</p>
    <ul class="list">
      {#each list as p (p.id)}
        <li>
          <span class="ellipsis"><strong>{p.name}</strong>{#if p.version}<span class="muted"> · {p.version}</span>{/if}</span>
          <span class="num">{p.size ? formatSize(p.size, loc) : ""}</span>
          {#if expert()}<code class="mono ellipsis cmd muted" title={command(p)}>{command(p)}</code>{/if}
        </li>
      {/each}
    </ul>
    <ol class="steps small">
      <li>{t("pg-confirm-step-run")}</li>
      <li>{t("pg-confirm-step-scan")}</li>
      <li>{t("pg-confirm-step-review")}</li>
    </ol>
    {#if machine}
      <p class="small note"><Icon name="shield" size={14} />{t("pg-confirm-machine")}</p>
    {/if}
    {#if cautions.length}
      <div class="warn">
        {#each cautions as p (p.id)}
          <p class="small"><Icon name="alert" size={14} /><b>{p.name}</b> — {t(`pg-caution-${p.caution}-hint`)}</p>
        {/each}
        <label class="ack small"><input type="checkbox" bind:checked={ack} data-pg-ack />{t("pg-confirm-ack")}</label>
      </div>
    {/if}
    {#if expert() && list.some((p) => p.quietUninstall)}
      <label class="ack small"><input type="checkbox" bind:checked={pg.quiet} />{t("pg-quiet")}</label>
    {/if}
    <footer>
      <button class="btn cancel" onclick={close}>{t("delete-cancel")}</button>
      <button class="btn danger" data-pg-go onclick={go} disabled={!list.length || (cautions.length > 0 && !ack)} data-effect>
        <Icon name="trash" size={16} />{t("pg-confirm-go", { count: list.length })}
      </button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--bg) 65%, transparent);
    backdrop-filter: blur(2px);
  }
  .dialog {
    width: min(660px, 94vw);
    max-height: 86vh;
    overflow: auto;
    padding: 22px 24px;
    box-shadow: var(--shadow);
  }
  h2 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0 0 8px;
    font-size: 1.2em;
  }
  .list {
    list-style: none;
    margin: 8px 0;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-height: 240px;
    overflow: auto;
  }
  .list li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px 10px;
    align-items: center;
    padding: 4px;
  }
  .cmd {
    grid-column: 1 / -1;
    font-size: 0.8em;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .steps {
    margin: 8px 0;
    padding-left: 20px;
  }
  .steps li {
    margin: 2px 0;
  }
  .note {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text-muted);
  }
  .warn {
    padding: 8px 10px;
    border-left: 3px solid var(--risk-caution);
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    margin: 8px 0;
  }
  .warn p {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 2px 0;
  }
  .small {
    font-size: 0.88em;
  }
  .ack {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 8px 0 0;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
