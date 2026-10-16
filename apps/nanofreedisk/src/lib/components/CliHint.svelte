<script lang="ts">
  // Expert mode (G2): the `nfd` command that does the same thing as the action next
  // to it. Renders nothing in Simple mode.
  import { t } from "../i18n.svelte";
  import { app, fail, toast } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { command }: { command: string } = $props();

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      toast(t("copied"), "ok");
    } catch (e) {
      fail(e);
    }
  }
</script>

{#if app.settings.mode === "expert"}
  <div class="cli" title={t("cli-hint-title")}>
    <span class="muted">{t("cli-hint")}</span>
    <code class="mono ellipsis">{command}</code>
    <button class="btn ghost icon" onclick={copy} title={t("action-copy-command")} aria-label={t("action-copy-command")}>
      <Icon name="copy" size={14} />
    </button>
  </div>
{/if}

<style>
  .cli {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font-size: 0.85em;
    padding: 2px 4px 2px 10px;
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }
  code {
    flex: 1;
    min-width: 0;
    color: var(--text);
  }
</style>
