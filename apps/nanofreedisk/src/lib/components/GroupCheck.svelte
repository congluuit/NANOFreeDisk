<script lang="ts">
  // Tri-state "select all in this group" checkbox (pack, vendor, project, duplicate
  // group…). Only selectable items are ticked — locked ones (the copy a duplicate
  // group keeps, blocked items, Protected / Keep) never.
  import { t } from "../i18n.svelte";
  import { groupState, setGroup, type SelItem } from "../selection";

  interface Props {
    items: SelItem[];
    group: string;
    picked: string[];
    onchange: (ids: string[]) => void;
    expert: boolean;
    name: string;
    busy?: boolean;
  }
  let { items, group, picked, onchange, expert, name, busy = false }: Props = $props();
  const state = $derived(groupState(items, group, picked, expert));
</script>

<label class="gcheck" title={t("sel-group", { name })}>
  <input
    type="checkbox"
    data-group-check={group}
    checked={state === "all"}
    indeterminate={state === "some"}
    disabled={busy || state === "empty"}
    aria-label={t("sel-group", { name })}
    onchange={() => onchange(setGroup(items, group, picked, state !== "all", expert))}
  />
</label>

<style>
  .gcheck {
    display: inline-flex;
    align-items: center;
  }
</style>
