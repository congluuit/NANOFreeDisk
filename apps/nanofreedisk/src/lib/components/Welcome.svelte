<script lang="ts">
  // First-run welcome: three short steps (what the app does, that nothing is deleted
  // without a confirmation and cleaned items come back for 7 days, Simple or Expert).
  // Skippable; once closed it never shows again (`settings.welcomed`), unless
  // Settings asks for it.
  import { t } from "../i18n.svelte";
  import { app, type Mode } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let step = $state(1);
  let mode = $state<Mode>(app.settings.mode);
  let dialog = $state<HTMLDivElement | null>(null);

  const ICONS = ["home", "shield", "settings"];

  function close(apply = false) {
    if (apply) app.settings.mode = mode;
    app.settings.welcomed = true;
  }

  $effect(() => {
    void step;
    dialog?.querySelector<HTMLElement>("[data-welcome-next]")?.focus();
  });
</script>

<div class="backdrop" role="presentation" onkeydown={(e) => e.key === "Escape" && close()}>
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="welcome-title" aria-describedby="welcome-body" data-welcome bind:this={dialog}>
    <div class="top">
      <span class="muted small">{t("welcome-step", { n: step })}</span>
      <button class="btn ghost small" data-welcome-skip onclick={() => close()}>{t("welcome-skip")}</button>
    </div>
    <span class="ic"><Icon name={ICONS[step - 1]} size={30} /></span>
    <h2 id="welcome-title">{t(`welcome-title-${step}`)}</h2>
    <p id="welcome-body" class="muted">{t(`welcome-body-${step}`)}</p>

    {#if step === 3}
      <div class="modes" role="radiogroup" aria-label={t("mode-label")}>
        {#each ["simple", "expert"] as const as m (m)}
          <button class="mode" class:on={mode === m} role="radio" aria-checked={mode === m} data-welcome-mode={m} onclick={() => (mode = m)}>
            <span class="radio" aria-hidden="true"></span>
            <span class="text">
              <strong>{t(`mode-${m}`)}</strong>
              <span class="muted small">{t(`welcome-${m}-sub`)}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}

    <div class="dots" aria-hidden="true">
      {#each [1, 2, 3] as n (n)}<span class:on={n === step}></span>{/each}
    </div>
    <footer>
      {#if step > 1}
        <button class="btn ghost" onclick={() => step--}>{t("welcome-back")}</button>
      {/if}
      <span class="grow"></span>
      {#if step < 3}
        <button class="btn primary" data-welcome-next onclick={() => step++}>{t("welcome-next")}</button>
      {:else}
        <button class="btn primary" data-welcome-next data-welcome-done onclick={() => close(true)}>{t("welcome-done")}</button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    backdrop-filter: blur(3px);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: min(480px, 92vw);
    max-height: 92vh;
    overflow: auto;
    padding: 18px 24px 20px;
    text-align: center;
    box-shadow: var(--shadow);
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    align-self: stretch;
  }
  .ic {
    display: grid;
    place-items: center;
    width: 64px;
    height: 64px;
    margin: 8px 0 12px;
    border-radius: 18px;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 1.25em;
  }
  p {
    margin: 0;
    max-width: 400px;
  }
  .modes {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-self: stretch;
    margin-top: 16px;
  }
  .mode {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    text-align: left;
  }
  .mode:hover {
    background: var(--hover);
  }
  .mode.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .radio {
    flex: none;
    width: 16px;
    height: 16px;
    margin-top: 2px;
    border-radius: 50%;
    border: 2px solid var(--text-muted);
  }
  .mode.on .radio {
    border: 5px solid var(--accent);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .dots {
    display: flex;
    gap: 6px;
    margin: 18px 0 4px;
  }
  .dots span {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--border);
  }
  .dots span.on {
    background: var(--accent);
  }
  footer {
    display: flex;
    gap: 8px;
    align-self: stretch;
    margin-top: 12px;
  }
  .grow {
    flex: 1;
  }
</style>
