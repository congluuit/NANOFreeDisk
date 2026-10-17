<script lang="ts">
  // "Delete anyway": the app suggested keeping something (an active AI session, git
  // state, a 🔒 rule, a guide-only folder…) and the user knows better. Hard
  // protections never reach this dialog. Nothing is deleted for good: a clean git
  // worktree goes with `git worktree remove`, the rest to the quarantine.
  import { blockedKey } from "../advice";
  import { advice, overrideAdvice, type OverrideRequest } from "../advice/state.svelte";
  import { formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import Icon from "./Icon.svelte";
  import VerdictBadge from "./VerdictBadge.svelte";

  let { req }: { req: OverrideRequest } = $props();
  let ack = $state(false);
  let dialog = $state<HTMLDivElement | null>(null);
  const loc = $derived(intlLocale());

  $effect(() => {
    dialog?.querySelector<HTMLElement>(".cancel")?.focus();
  });

  function close() {
    if (!advice.running) advice.override = null;
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={(e) => e.key === "Escape" && close()}>
  <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="override-title" data-override-confirm bind:this={dialog}>
    <h2 id="override-title"><Icon name="alert" size={20} />{t("advice-override-title")}</h2>
    <p>{t("advice-override-body", { count: req.ids.length, size: formatSize(req.bytes, loc) })}</p>
    <ul class="list">
      {#each req.rows as r (r.key)}
        <li>
          <VerdictBadge verdict={r.verdict} />
          <span class="ellipsis mono" title={r.location ?? r.label}>{r.location || r.label}</span>
          <span class="num">{formatSize(r.bytes, loc)}</span>
        </li>
      {/each}
    </ul>
    <div class="why">
      <b>{t("advice-override-why")}:</b>
      {req.why}
      {#if req.blocked}<p class="blocked"><Icon name="shield" size={13} />{t(blockedKey(req.blocked), { code: req.blocked })}</p>{/if}
    </div>
    <p class="small">{t("advice-override-ways")}</p>
    <label class="ack"><input type="checkbox" data-override-ack bind:checked={ack} />{t("advice-override-ack")}</label>
    <footer>
      <button class="btn cancel" onclick={close} disabled={advice.running}>{t("delete-cancel")}</button>
      <button class="btn danger" data-override-go onclick={() => ack && overrideAdvice(req)} disabled={!ack || advice.running} data-effect title={t("advice-override-hint")}>
        <Icon name="trash" size={16} />{t("advice-override-go")}
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
    width: min(640px, 94vw);
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
    color: var(--risk-caution);
  }
  .list {
    list-style: none;
    margin: 8px 0;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-height: 200px;
    overflow: auto;
  }
  .list li {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 3px 4px;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .why {
    font-size: 0.9em;
    padding: 8px 10px;
    border-left: 3px solid var(--risk-caution);
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .blocked {
    display: flex;
    gap: 5px;
    align-items: center;
    margin: 4px 0 0;
    color: var(--risk-caution);
  }
  .small {
    font-size: 0.86em;
  }
  .ack {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 6px;
    font-weight: 600;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
</style>
