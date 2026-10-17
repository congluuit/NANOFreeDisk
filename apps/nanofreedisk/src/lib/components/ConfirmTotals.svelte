<script lang="ts">
  // What a confirmation repeats from the selection bar: count and size per risk
  // level, what is likely skipped (and not counted), and — when 🟡 Caution or
  // 🟠 Advanced items are included — a tick the user must give before going on.
  import { formatCount, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { needsAck, type SelSummary } from "../selection";
  import Icon from "./Icon.svelte";

  let { summary, ack = $bindable(false) }: { summary: SelSummary; ack?: boolean } = $props();
  const loc = $derived(intlLocale());
</script>

<div class="totals" data-confirm-totals>
  <p class="total">
    <strong>{t("sel-confirm-total", { count: formatCount(summary.count, loc), size: formatSize(summary.bytes, loc) })}</strong>
  </p>
  {#if summary.byLevel.length}
    <ul class="levels">
      {#each summary.byLevel as l (l.level)}
        <li class={l.level} data-confirm-level={l.level}>
          <span class="dot"></span>{t(`clean-level-${l.level}`)}
          <span class="grow"></span>
          <span class="num">{t("sel-confirm-level", { count: formatCount(l.count, loc), size: formatSize(l.bytes, loc) })}</span>
        </li>
      {/each}
    </ul>
  {/if}
  {#if summary.skipCount}
    <p class="skip small"><Icon name="alert" size={14} />{t("sel-skip", { count: summary.skipCount, apps: summary.skipReasons.join(", "), size: formatSize(summary.skipBytes, loc) })}</p>
  {/if}
  {#if needsAck(summary)}
    <label class="ack small"><input type="checkbox" data-confirm-ack bind:checked={ack} />{t("sel-ack")}</label>
  {/if}
</div>

<style>
  .totals p {
    margin: 6px 0;
  }
  .levels {
    list-style: none;
    margin: 4px 0;
    padding: 6px 8px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .levels li {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 2px 0;
  }
  .grow {
    flex: 1;
  }
  .small {
    font-size: 0.86em;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    display: inline-block;
  }
  .safe .dot {
    background: var(--risk-safe);
  }
  .caution .dot {
    background: var(--risk-caution);
  }
  .advanced .dot {
    background: var(--risk-advanced);
  }
  .skip {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--risk-caution);
  }
  .ack {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 8px;
    color: var(--risk-caution);
  }
</style>
