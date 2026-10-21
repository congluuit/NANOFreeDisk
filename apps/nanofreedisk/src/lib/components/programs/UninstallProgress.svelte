<script lang="ts">
  // The running uninstall: the program's icon inside a spinning ring (the
  // vendor's uninstaller gives no progress to show, so the ring only says "still
  // working"), the three steps with the current one lit, how long it has been
  // running, how many of the uninstaller's processes are still alive and what
  // comes next in the queue.
  import { t } from "../../i18n.svelte";
  import { pg, programOf, stopWaiting } from "../../programs/state.svelte";
  import AppIcon from "./AppIcon.svelte";
  import Icon from "../Icon.svelte";

  const c = $derived(pg.current!);
  const scanning = $derived(c.phase === "scanning");
  const step = $derived(scanning ? 1 : 0);
  const STEPS = ["run", "scan", "review"] as const;

  // elapsed time of the current program
  let started = $state(Date.now());
  let now = $state(Date.now());
  let lastId = "";
  $effect(() => {
    if (c.id !== lastId) {
      lastId = c.id;
      started = Date.now();
      now = started;
    }
  });
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  const elapsed = $derived.by(() => {
    const s = Math.max(0, Math.floor((now - started) / 1000));
    const m = Math.floor(s / 60);
    return `${m}:${String(s % 60).padStart(2, "0")}`;
  });
  const next = $derived(
    pg.queue
      .map((id) => programOf(id)?.name)
      .filter(Boolean)
      .slice(0, 3)
      .join(", "),
  );
</script>

<div class="card uninstalling" class:scanning aria-live="polite" data-pg-running data-pg-phase={c.phase}>
  <div class="ring" role="progressbar" aria-label={scanning ? t("pg-scanning", { name: c.name }) : t("pg-running", { name: c.name })}>
    <svg viewBox="0 0 72 72" aria-hidden="true">
      <circle class="track" cx="36" cy="36" r="32" />
      <circle class="arc" cx="36" cy="36" r="32" pathLength="100" />
      <circle class="arc second" cx="36" cy="36" r="32" pathLength="100" />
    </svg>
    <span class="halo"></span>
    <span class="icon"><AppIcon id={c.id} name={c.name} size={36} /></span>
  </div>

  <div class="body">
    <strong class="title">{scanning ? t("pg-scanning", { name: c.name }) : t("pg-running", { name: c.name })}</strong>
    <p class="muted hint">{scanning ? t("pg-scanning-hint") : t("pg-running-hint")}</p>

    <ol class="steps">
      {#each STEPS as s, i (s)}
        <li class:done={i < step} class:now={i === step}>
          <span class="dot">{#if i < step}<Icon name="check" size={12} />{:else}{i + 1}{/if}</span>
          <span class="label">{t(`pg-step-${s}`)}</span>
        </li>
      {/each}
    </ol>

    <p class="meta muted">
      <span title={t("pg-elapsed-hint")}><Icon name="clock" size={13} />{elapsed}</span>
      {#if !scanning && c.alive > 0}<span>{t("pg-alive", { count: c.alive })}</span>{/if}
      {#if pg.queue.length}<span class="ellipsis" title={next}>{t("pg-queue", { count: pg.queue.length })}{next ? ` · ${t("pg-next", { names: next })}` : ""}</span>{/if}
    </p>
  </div>

  {#if !scanning}
    <button class="btn" onclick={stopWaiting} data-pg-finished title={t("pg-finished-hint")}>
      <Icon name="check" size={16} />{t("pg-finished")}
    </button>
  {/if}
</div>

<style>
  .uninstalling {
    display: flex;
    gap: 18px;
    align-items: center;
    padding: 16px 18px;
    border-left: 4px solid var(--accent);
  }
  .uninstalling.scanning {
    border-left-color: var(--risk-safe);
  }
  .ring {
    position: relative;
    flex: none;
    width: 72px;
    height: 72px;
    display: grid;
    place-items: center;
  }
  .ring svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    animation: spin 1.15s linear infinite;
  }
  .track {
    fill: none;
    stroke: var(--border);
    stroke-width: 5;
  }
  .arc {
    fill: none;
    stroke: var(--accent);
    stroke-width: 5;
    stroke-linecap: round;
    stroke-dasharray: 28 72;
    animation: breathe 2.3s ease-in-out infinite;
  }
  .arc.second {
    stroke-dasharray: 8 92;
    stroke-dashoffset: -50;
    opacity: 0.45;
    animation: none;
  }
  .scanning .arc {
    stroke: var(--risk-safe);
  }
  .scanning svg {
    animation-duration: 0.8s;
  }
  .halo {
    position: absolute;
    inset: 12px;
    border-radius: 50%;
    background: var(--accent-soft);
    animation: pulse 2.3s ease-in-out infinite;
  }
  .icon {
    position: relative;
    display: grid;
    place-items: center;
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .title {
    font-size: 1.05em;
  }
  .hint {
    margin: 0;
    font-size: 0.88em;
  }
  .steps {
    display: flex;
    gap: 6px;
    align-items: center;
    list-style: none;
    margin: 8px 0 4px;
    padding: 0;
    flex-wrap: wrap;
  }
  .steps li {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 0.84em;
    color: var(--text-muted);
  }
  .steps li + li::before {
    content: "";
    width: 22px;
    height: 2px;
    border-radius: 1px;
    background: var(--border);
  }
  .steps li.done + li::before {
    background: var(--risk-safe);
  }
  .dot {
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    border: 2px solid var(--border);
    font-size: 0.8em;
    font-weight: 700;
  }
  .done {
    color: var(--risk-safe-ink);
  }
  .done .dot {
    border-color: var(--risk-safe);
    background: var(--risk-safe);
    color: var(--surface);
  }
  .now {
    color: var(--text);
    font-weight: 600;
  }
  .now .dot {
    border-color: var(--accent);
    color: var(--accent);
    animation: pulse 1.6s ease-in-out infinite;
  }
  .scanning .now .dot {
    border-color: var(--risk-safe);
    color: var(--risk-safe-ink);
  }
  .meta {
    display: flex;
    gap: 6px 14px;
    flex-wrap: wrap;
    margin: 2px 0 0;
    font-size: 0.84em;
    min-width: 0;
  }
  .meta span {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    font-variant-numeric: tabular-nums;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes breathe {
    50% {
      stroke-dasharray: 52 48;
    }
  }
  @keyframes pulse {
    50% {
      opacity: 0.55;
      transform: scale(0.94);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .ring svg {
      animation-duration: 4s;
    }
    .arc,
    .halo,
    .now .dot {
      animation: none;
    }
  }
</style>
