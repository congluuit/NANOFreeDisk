<script lang="ts">
  // DevCache → AI leftovers (PLAN §13.2 C10, docs/AI_LEFTOVERS.md): what AI coding
  // agents left behind per session — worktrees, Temp scratchpads, browser snapshots,
  // screenshots, test output, old Playwright browsers — grouped project → tool, each
  // with the session that made it. Conversations are never offered; leftovers of
  // sessions still open or active recently are listed but not pre-selected.
  // Folders go to the quarantine; worktrees are removed by git afterwards.
  import { formatCount, formatDate, formatSize } from "../../format";
  import { i18n, intlLocale, t } from "../../i18n.svelte";
  import { app, fail } from "../../state.svelte";
  import { reasonText } from "../../clean/state.svelte";
  import { devApi } from "../../devcache/api";
  import { aiBytes, aiPreselected } from "../../devcache/ai";
  import { cleanAi, confirmAi, dev, loadAi } from "../../devcache/state.svelte";
  import type { AiItem, Artifact } from "../../devcache/types";
  import CliHint from "../CliHint.svelte";
  import Icon from "../Icon.svelte";

  const loc = $derived(intlLocale());
  const now = () => Math.floor(Date.now() / 1000);

  $effect(() => {
    if (!dev.ai && !dev.aiLoading && !dev.running) void loadAi();
  });

  const r = $derived(dev.ai);
  /** project (or "" for Temp / tool stores) → tool → items */
  const groups = $derived.by(() => {
    const out = new Map<string, Map<string, AiItem[]>>();
    for (const i of r?.items ?? []) {
      const p = i.project ?? "";
      if (!out.has(p)) out.set(p, new Map());
      const tools = out.get(p)!;
      if (!tools.has(i.tool)) tools.set(i.tool, []);
      tools.get(i.tool)!.push(i);
    }
    return [...out.entries()].sort(([a], [b]) => (a === "" ? 1 : b === "" ? -1 : a.localeCompare(b)));
  });
  const keptOf = (project: string) => (r?.kept ?? []).filter((k) => (k.project ?? "") === project);
  const pickedBytes = $derived(aiBytes(r, dev.aiPicked));
  const finished = $derived(aiPreselected(r));
  const finishedBytes = $derived(aiBytes(r, finished));

  function toggle(path: string) {
    dev.aiPicked = dev.aiPicked.includes(path) ? dev.aiPicked.filter((p) => p !== path) : [...dev.aiPicked, path];
  }
  const impact = (i: AiItem) => (i18n.locale === "vi" ? i.impactVi : i.impact);
  const blockedText = (code: string) => reasonText(code);
  const days = (ts: number) => Math.max(0, Math.floor((now() - ts) / 86_400));
  function sessionText(i: AiItem): string {
    const s = i.session!;
    const title = s.title ?? t("ail-session-untitled", { tool: s.tool, id: s.id.slice(0, 8) });
    return t("ail-session", { title, days: days(s.lastActive) });
  }
  function wtState(i: AiItem): string {
    const w = i.worktree!;
    if (!w.registered) return t("ail-wt-orphan");
    if (w.prunable) return t("ail-wt-prunable");
    const merged = w.merged ? t("ail-wt-merged", { branch: w.defaultBranch ?? "main" }) : w.pushed ? t("ail-wt-pushed") : t("ail-wt-unmerged");
    return `${w.branch ?? t("ail-wt-detached")} · ${merged} · ${t("ail-wt-changes", { count: w.changes })}`;
  }
  const ignoredBytes = (i: AiItem) => (i.worktree?.ignored ?? []).reduce((s, x) => s + x.bytes, 0);
  async function go() {
    await cleanAi(dev.aiPicked);
  }
  function artBlocked(a: Artifact): string {
    return a.blocked === "tracked" ? t("dev-art-tracked") : a.blocked === "not-ignored" ? t("dev-art-not-ignored") : reasonText(a.blocked);
  }
</script>

<div class="card dc-card">
  <div class="dc-row top">
    <span class="lead"><Icon name="agent" size={20} /></span>
    <p class="dc-grow intro">{t("ail-intro", { days: app.settings.keepRecentDays })}</p>
  </div>
  <div class="dc-row actions">
    <span class="dc-grow"></span>
    <button class="btn" onclick={() => loadAi()} disabled={dev.aiLoading || !!dev.running} data-effect title={t("ail-rescan-hint")}>
      <Icon name="refresh" size={16} />{t("ail-rescan")}
    </button>
    {#if dev.aiLoading}<button class="btn ghost" onclick={() => devApi.cancel().catch(fail)}>{t("clean-cancel")}</button>{/if}
    <button class="btn primary" data-ai-finished disabled={!finished.length || dev.aiLoading || !!dev.running} onclick={() => confirmAi(finished)} data-effect title={t("ail-finished-hint")}>
      <Icon name="clean" size={16} />{t("ail-finished", { size: formatSize(finishedBytes, loc) })}
    </button>
  </div>
</div>

{#if dev.aiLoading}
  <div class="card dc-loading"><Icon name="search" size={22} /><span>{t("ail-loading")}</span></div>
{:else if r}
  <div class="card dc-note">
    <Icon name="shield" size={18} />
    <span class="dc-small">{t("ail-summary", { sessions: formatCount(r.sessions, loc), projects: r.projects.length, size: formatSize(r.totalBytes, loc) })}</span>
    {#if !r.git}<span class="dc-warn dc-small"><Icon name="alert" size={14} />{t("ail-no-git")}</span>{/if}
    {#if r.cancelled}<span class="dc-warn dc-small">{t("clean-was-cancelled")}</span>{/if}
  </div>
  {#each groups as [project, tools] (project)}
    <div class="card dc-card proj" data-ai-project={project || "temp"}>
      <div class="dc-row">
        <h2 class="ellipsis" title={project}>{project ? project.split(/[\\/]/).pop() : t("ail-no-project")}</h2>
        <span class="dc-grow"></span>
        <b class="num">{formatSize([...tools.values()].flat().reduce((s, i) => s + (i.blocked ? 0 : i.bytes), 0), loc)}</b>
      </div>
      {#if project}<p class="muted dc-small mono ellipsis" title={project}>{project}</p>{/if}
      {#each [...tools.entries()] as [tool, items] (tool)}
        <h3 class="tool">{tool}</h3>
        <ul class="dc-list">
          {#each items as i (i.path)}
            <li class="dc-item" class:blocked={!!i.blocked} data-ai-item={i.rel} data-ai-kind={i.kind}>
              <label title={i.blocked ? blockedText(i.blocked) : t("clean-pick-hint")}>
                <input type="checkbox" checked={dev.aiPicked.includes(i.path)} disabled={!!i.blocked || !!dev.running} onchange={() => toggle(i.path)} />
              </label>
              <div class="body">
                <div class="dc-row">
                  <span class="dc-badge {i.level}">{t(`clean-level-${i.level}`)}</span>
                  <code class="mono ellipsis" title={i.path}>{i.rel}</code>
                  <button class="btn ghost icon" onclick={() => devApi.reveal(i.path).catch(fail)} title={t("dev-reveal")} aria-label={t("dev-reveal")} disabled={i.worktree?.prunable}><Icon name="reveal" size={14} /></button>
                </div>
                {#if i.session}
                  <p class="dc-small session">
                    <Icon name="agent" size={13} />{sessionText(i)}
                    {#if i.session.open}<span class="dc-badge muted" title={t("ail-open-hint")}>{t("ail-open")}</span>{/if}
                    {#if i.session.recent}<span class="dc-badge safe">{t("dev-repo-recent")}</span>{/if}
                  </p>
                {/if}
                {#if i.worktree}
                  <p class="dc-small muted">{wtState(i)}{#if i.worktree.ignored.length}{` · ${t("ail-wt-ignored", { size: formatSize(ignoredBytes(i), loc) })}`}{/if}</p>
                {/if}
                <p class="dc-small muted">{impact(i)}</p>
                {#if i.blocked}
                  <p class="dc-small dc-warn"><Icon name="lock" size={13} />{blockedText(i.blocked)}</p>
                {:else if i.keep}
                  <p class="dc-small keep"><Icon name="clock" size={13} />{t(`ail-keep-${i.keep}`, { days: r.keepRecentDays })}</p>
                {/if}
                {#if i.worktree?.command && app.settings.mode === "expert"}
                  <span class="dc-code dc-small"><code class="mono">{i.worktree.command}</code></span>
                {/if}
                {#if i.worktree?.artifacts.length}
                  <p class="dc-small muted">{t("ail-wt-artifacts")}</p>
                  <ul class="dc-list arts">
                    {#each i.worktree.artifacts as a (a.path)}
                      <li class="dc-item" class:blocked={!!a.blocked}>
                        <label title={a.blocked ? artBlocked(a) : t("clean-pick-hint")}>
                          <input type="checkbox" checked={dev.aiPicked.includes(a.path)} disabled={!!a.blocked || !!dev.running} onchange={() => toggle(a.path)} />
                        </label>
                        <div>
                          <span class="dc-badge {a.level}">{t(`clean-level-${a.level}`)}</span>
                          <code class="mono">{a.rel}</code>
                          <span class="muted dc-small">{t(`dev-kind-${a.kind}`)}</span>
                          {#if a.blocked}<p class="dc-small dc-warn"><Icon name="lock" size={13} />{artBlocked(a)}</p>{/if}
                        </div>
                        <div class="dc-size"><b class="num">{formatSize(a.bytes, loc)}</b></div>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </div>
              <div class="dc-size">
                <b class="num">{formatSize(i.bytes, loc)}{i.truncated ? "+" : ""}</b>
                <span class="muted dc-small">{t("clean-files", { count: formatCount(i.files, loc) })}</span>
                {#if i.mtime}<span class="muted dc-small">{formatDate(i.mtime, loc)}</span>{/if}
              </div>
            </li>
          {/each}
        </ul>
      {/each}
      {#each keptOf(project) as k (k.path)}
        <p class="dc-small kept" data-ai-kept={k.what}><Icon name="lock" size={13} />{t("ail-kept", { what: k.what, tool: k.tool })}</p>
      {/each}
    </div>
  {:else}
    <div class="card dc-loading"><Icon name="check" size={22} /><span>{t("ail-none")}</span></div>
  {/each}

  <div class="card dc-footer">
    <span>{t("dev-repos-selected", { count: dev.aiPicked.length, size: formatSize(pickedBytes, loc) })}</span>
    <span class="dc-grow"></span>
    <button class="btn danger" data-ai-clean disabled={!dev.aiPicked.length || !!dev.running} onclick={() => confirmAi(dev.aiPicked)} data-effect title={t("ail-clean-hint")}>
      <Icon name="clean" size={16} />{t("ail-clean")}
    </button>
  </div>
  <CliHint command={`nfd ai-leftovers clean --keep-recent ${app.settings.keepRecentDays}${app.settings.repoRoot ? ` --root "${app.settings.repoRoot}"` : ""} --apply`} />
{/if}

{#if dev.aiConfirm}
  {@const plan = dev.aiConfirm}
  <div class="dc-backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (dev.aiConfirm = null)} onkeydown={(e) => e.key === "Escape" && (dev.aiConfirm = null)}>
    <div class="dc-dialog card" role="alertdialog" aria-modal="true" aria-labelledby="ai-confirm-title" data-ai-confirm>
      <h2 id="ai-confirm-title"><Icon name="clean" size={20} />{t("ail-confirm-title")}</h2>
      <p>{t("ail-confirm-body", { count: plan.folders, size: formatSize(plan.bytes, loc) })}</p>
      {#if plan.commands.length}
        <p class="dc-small">{t("ail-confirm-git", { count: plan.commands.length })}</p>
        <ul class="dc-confirm-list">
          {#each plan.commands as c (c)}<li><code class="mono cmd">{c}</code></li>{/each}
        </ul>
      {/if}
      <p class="muted dc-small">{t("ail-confirm-note")}</p>
      <footer>
        <button class="btn" onclick={() => (dev.aiConfirm = null)}>{t("delete-cancel")}</button>
        <button class="btn danger" onclick={go} data-effect title={t("ail-confirm-note")}><Icon name="clean" size={16} />{t("clean-confirm-go")}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .top {
    align-items: flex-start;
    flex-wrap: nowrap;
  }
  .lead {
    color: var(--accent);
    display: flex;
    padding-top: 2px;
  }
  .intro {
    margin: 0;
    max-width: 90ch;
  }
  .actions {
    margin-top: 10px;
  }
  .proj h2 {
    margin: 0;
    font-size: 1.05em;
  }
  .dc-card p {
    margin: 4px 0 0;
  }
  .tool {
    margin: 12px 0 0;
    font-size: 0.92em;
    color: var(--text-muted);
  }
  .body {
    min-width: 0;
  }
  .session {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
  .keep {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text-muted);
  }
  .kept {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--risk-protected);
  }
  .arts {
    margin-top: 4px;
  }
  .blocked {
    opacity: 0.8;
  }
  .cmd {
    white-space: normal;
    word-break: break-all;
  }
</style>
