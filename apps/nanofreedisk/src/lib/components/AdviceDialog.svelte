<script lang="ts">
  // Confirmation of a suggestions clean: what goes, per verdict, the ways it goes
  // (quarantine, git, Recycle Bin) and — with 🟡 groups — a tick the user must give.
  import { advice, cleanAdvice, type ConfirmRequest } from "../advice/state.svelte";
  import { formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { needsAck, summarize } from "../selection";
  import { expert } from "../state.svelte";
  import ConfirmTotals from "./ConfirmTotals.svelte";
  import Icon from "./Icon.svelte";
  import VerdictBadge from "./VerdictBadge.svelte";

  let { req }: { req: ConfirmRequest } = $props();
  let ack = $state(false);
  let dialog = $state<HTMLDivElement | null>(null);
  const loc = $derived(intlLocale());
  const summary = $derived(summarize(req.items, req.keys, expert()));
  const mustAck = $derived(needsAck(summary));

  $effect(() => {
    dialog?.querySelector<HTMLElement>(".cancel")?.focus();
  });

  function close() {
    if (!advice.running) advice.confirm = null;
  }

  function go() {
    if (mustAck && !ack) return;
    void cleanAdvice(req, mustAck && ack);
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={(e) => e.key === "Escape" && close()}>
  <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="advice-confirm-title" data-advice-confirm bind:this={dialog}>
    <h2 id="advice-confirm-title"><Icon name="clean" size={20} />{t("advice-confirm-title")}</h2>
    <p>{t("advice-confirm-body", { count: req.rows.length, size: formatSize(summary.bytes, loc) })}</p>
    <ul class="list">
      {#each req.rows as r (r.key)}
        <li data-confirm-row={r.key}>
          <VerdictBadge verdict={r.verdict} />
          <span class="ellipsis" title={r.impact}>{r.label}{r.location ? ` — ${r.location}` : ""}</span>
          <span class="num">{formatSize(r.bytes, loc)}</span>
        </li>
      {/each}
    </ul>
    <ConfirmTotals {summary} bind:ack />
    <p class="small">{t("advice-confirm-ways")}</p>
    <p class="muted small">{t("advice-confirm-note")}</p>
    <footer>
      <button class="btn cancel" onclick={close} disabled={advice.running}>{t("delete-cancel")}</button>
      <button class="btn danger" data-advice-go onclick={go} disabled={(mustAck && !ack) || advice.running || !req.keys.length} data-effect title={t("advice-confirm-note")}>
        <Icon name="clean" size={16} />{t("advice-confirm-go")}
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
  }
  .list {
    list-style: none;
    margin: 8px 0;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-height: 220px;
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
  .small {
    font-size: 0.86em;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
</style>
