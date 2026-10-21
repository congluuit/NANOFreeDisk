<script lang="ts">
  // macOS without Full Disk Access: the system hides Mail, Messages, Safari and
  // other apps' data, so scans of the home folder or the disk skip them. Says how
  // to grant it, in the views that scan (until dismissed, once per launch).
  import { api } from "../api";
  import { t } from "../i18n.svelte";
  import { app } from "../state.svelte";
  import { currentOs } from "../platform";
  import Icon from "./Icon.svelte";

  const SCANNING_VIEWS = ["home", "analyze", "duplicates", "oldlarge", "tidy", "devcache"];
  let dismissed = $state(false);
  const shown = $derived(
    !dismissed &&
      currentOs(app.info?.os) === "macos" &&
      app.info?.fullDiskAccess === false &&
      SCANNING_VIEWS.includes(app.view),
  );
</script>

{#if shown}
  <aside class="fda card" role="note" aria-label={t("mac-fda-title")} data-mac-fda>
    <Icon name="lock" size={18} />
    <div class="text">
      <strong>{t("mac-fda-title")}</strong>
      <p class="muted small">{t("mac-fda-body")}</p>
      <div class="row">
        <button class="btn primary" onclick={() => api.openPrivacySettings().catch(() => {})} title={t("mac-fda-open-hint")}>
          {t("mac-fda-open")}
        </button>
        <button class="btn ghost" onclick={() => (dismissed = true)}>{t("mac-fda-dismiss")}</button>
      </div>
    </div>
  </aside>
{/if}

<style>
  .fda {
    position: fixed;
    right: 16px;
    top: 16px;
    z-index: 40;
    max-width: 420px;
    display: flex;
    gap: 10px;
    padding: 12px 14px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-left: 4px solid var(--accent);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow);
    color: var(--text);
  }
  .text p {
    margin: 4px 0 8px;
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
</style>
