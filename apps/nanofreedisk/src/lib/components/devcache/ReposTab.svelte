<script lang="ts">
  // DevCache → Repo (C3 + C9): git repositories and projects under a folder, their
  // `.git` size, last commit, uncommitted changes and build output found by marker
  // files. Generic names need git's "ignored", folders with tracked files are never
  // offered, projects active in the last N days are not pre-selected; cleaning moves
  // the folders to the quarantine (Activity log → Restore).
  import { pickFolder } from "../../api";
  import { formatCount, formatDate, formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { app, fail } from "../../state.svelte";
  import { reasonText } from "../../clean/state.svelte";
  import { devApi } from "../../devcache/api";
  import { cleanRepos, copyText, dev, loadRepos } from "../../devcache/state.svelte";
  import type { Artifact } from "../../devcache/types";
  import CliHint from "../CliHint.svelte";
  import Icon from "../Icon.svelte";

  const loc = $derived(intlLocale());
  let root = $state(app.settings.repoRoot);

  async function choose() {
    const p = await pickFolder(t("dev-repos-pick"));
    if (!p) return;
    root = p;
    app.settings.repoRoot = p;
    await loadRepos(p);
  }
  function scan() {
    if (!root) return;
    app.settings.repoRoot = root;
    void loadRepos(root);
  }
  $effect(() => {
    if (!dev.repos && !dev.reposLoading && app.settings.repoRoot && root === app.settings.repoRoot) void loadRepos(root);
  });

  const all = $derived((dev.repos?.repos ?? []).flatMap((r) => r.artifacts));
  const picked = $derived(all.filter((a) => dev.repoPicked.includes(a.path)));
  const pickedBytes = $derived(picked.reduce((s, a) => s + a.bytes, 0));

  function toggle(a: Artifact) {
    dev.repoPicked = dev.repoPicked.includes(a.path) ? dev.repoPicked.filter((p) => p !== a.path) : [...dev.repoPicked, a.path];
  }
  function blockedText(code: string): string {
    return code === "tracked" ? t("dev-art-tracked") : code === "not-ignored" ? t("dev-art-not-ignored") : reasonText(code);
  }
  async function go() {
    dev.repoConfirm = false;
    await cleanRepos(picked.map((a) => a.path));
  }
  const gc = (path: string) => `git -C "${path}" gc`;
</script>

<div class="card dc-card">
  <div class="dc-row">
    <input class="root mono" bind:value={root} placeholder={t("dev-repos-root")} aria-label={t("dev-repos-root")} onkeydown={(e) => e.key === "Enter" && scan()} />
    <button class="btn" onclick={choose} disabled={dev.reposLoading}><Icon name="folder" size={16} />{t("dev-repos-choose")}</button>
    <button class="btn primary" onclick={scan} disabled={!root || dev.reposLoading} data-effect title={t("dev-repos-scan-hint")}><Icon name="search" size={16} />{t("dev-repos-scan")}</button>
    {#if dev.reposLoading}<button class="btn ghost" onclick={() => devApi.cancel().catch(fail)}>{t("clean-cancel")}</button>{/if}
  </div>
  <p class="muted dc-small">{t("dev-repos-policy", { days: app.settings.keepRecentDays })}</p>
</div>

{#if dev.reposLoading}
  <div class="card dc-loading"><Icon name="search" size={22} /><span>{t("dev-repos-loading")}</span></div>
{:else if dev.repos}
  {@const rs = dev.repos}
  <div class="card dc-note">
    <Icon name="folder" size={18} />
    <span class="dc-small">{t("dev-repos-summary", { count: rs.repos.length, folders: formatCount(rs.folders, loc) })}</span>
    {#if !rs.git}<span class="dc-warn dc-small"><Icon name="alert" size={14} />{t("dev-repos-no-git")}</span>{/if}
    {#if rs.cancelled}<span class="dc-warn dc-small">{t("clean-was-cancelled")}</span>{/if}
  </div>
  {#each rs.repos as r (r.path)}
    <div class="card dc-card repo" data-repo={r.name}>
      <div class="dc-row">
        <h2>{r.name}</h2>
        <span class="dc-badge muted">{r.git ? "git" : t("dev-repo-no-git")}</span>
        {#each r.kinds as k (k)}<span class="dc-badge muted">{k}</span>{/each}
        {#if r.dirty}<span class="dc-badge caution" title={t("dev-repo-dirty-hint")}>{t("dev-repo-dirty")}</span>{/if}
        {#if r.recent}<span class="dc-badge safe" title={t("dev-repo-recent-hint", { days: app.settings.keepRecentDays })}>{t("dev-repo-recent")}</span>{/if}
        {#if r.stale}<span class="dc-badge muted">{t("dev-repo-stale")}</span>{/if}
        <span class="dc-grow"></span>
        <b class="num">{formatSize(r.artifactBytes, loc)}</b>
        <button class="btn ghost icon" onclick={() => devApi.reveal(r.path).catch(fail)} title={t("dev-reveal")} aria-label={t("dev-reveal")}><Icon name="reveal" size={14} /></button>
      </div>
      <p class="muted dc-small mono ellipsis" title={r.path}>{r.path}</p>
      <p class="muted dc-small">
        {#if r.git}.git {formatSize(r.gitBytes, loc)} · {/if}
        {#if r.lastCommit}{t("dev-repo-commit", { date: formatDate(r.lastCommit, loc) })} {r.lastCommitSubject ?? ""} · {/if}
        {t("dev-repo-activity", { date: r.lastActivity ? formatDate(r.lastActivity, loc) : "—" })}
      </p>
      {#if r.git && app.settings.mode === "expert"}
        <span class="dc-code dc-small" title={t("dev-gc-hint")}><span class="muted">{t("dev-gc")}</span><code class="mono">{gc(r.path)}</code>
          <button class="btn ghost icon" onclick={() => copyText(gc(r.path))} title={t("action-copy-command")} aria-label={t("action-copy-command")}><Icon name="copy" size={14} /></button>
        </span>
      {/if}
      <ul class="dc-list arts">
        {#each r.artifacts as a (a.path)}
          <li class="dc-item" class:blocked={!!a.blocked}>
            <label title={a.blocked ? blockedText(a.blocked) : t("clean-pick-hint")}>
              <input type="checkbox" checked={dev.repoPicked.includes(a.path)} disabled={!!a.blocked || !!dev.running} onchange={() => toggle(a)} />
            </label>
            <div>
              <div class="dc-row">
                <span class="dc-badge {a.level}">{t(`clean-level-${a.level}`)}</span>
                <code class="mono">{a.rel}</code>
                <span class="muted dc-small">{t(`dev-kind-${a.kind}`)}</span>
              </div>
              <p class="dc-small muted">{t(`dev-art-impact-${a.level === "caution" ? "caution" : "safe"}`)}</p>
              {#if a.blocked}<p class="dc-small dc-warn"><Icon name="lock" size={13} />{blockedText(a.blocked)}</p>{/if}
            </div>
            <div class="dc-size">
              <b class="num">{formatSize(a.bytes, loc)}{a.truncated ? "+" : ""}</b>
              <span class="muted dc-small">{t("clean-files", { count: formatCount(a.files, loc) })}</span>
            </div>
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <div class="card dc-loading"><Icon name="check" size={22} /><span>{t("dev-repos-none")}</span></div>
  {/each}

  <div class="card dc-footer">
    <span>{t("dev-repos-selected", { count: picked.length, size: formatSize(pickedBytes, loc) })}</span>
    <span class="dc-grow"></span>
    <button class="btn danger" disabled={!picked.length || !!dev.running} onclick={() => (dev.repoConfirm = true)} data-effect title={t("dev-repos-clean-hint")}>
      <Icon name="clean" size={16} />{t("dev-repos-clean")}
    </button>
  </div>
  <CliHint command={`nfd repos clean "${rs.root}" --keep-recent ${app.settings.keepRecentDays} --apply`} />
{/if}

{#if dev.repoConfirm}
  <div class="dc-backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (dev.repoConfirm = false)} onkeydown={(e) => e.key === "Escape" && (dev.repoConfirm = false)}>
    <div class="dc-dialog card" role="alertdialog" aria-modal="true" aria-labelledby="repo-confirm-title">
      <h2 id="repo-confirm-title"><Icon name="clean" size={20} />{t("dev-repos-confirm-title")}</h2>
      <p>{t("dev-repos-confirm-body", { count: picked.length, size: formatSize(pickedBytes, loc) })}</p>
      <ul class="dc-confirm-list">
        {#each picked as a (a.path)}
          <li><span class="dc-badge {a.level}">{t(`clean-level-${a.level}`)}</span><span class="mono ellipsis" title={a.path}>{a.path}</span><span class="num">{formatSize(a.bytes, loc)}</span></li>
        {/each}
      </ul>
      <p class="muted dc-small">{t("clean-confirm-note")}</p>
      <footer>
        <button class="btn" onclick={() => (dev.repoConfirm = false)}>{t("delete-cancel")}</button>
        <button class="btn danger" onclick={go} data-effect title={t("clean-confirm-note")}><Icon name="clean" size={16} />{t("clean-confirm-go")}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .root {
    flex: 1;
    min-width: 240px;
    padding: 7px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text);
  }
  .dc-card p {
    margin: 6px 0 0;
  }
  .repo h2 {
    margin: 0;
    font-size: 1.05em;
  }
  .arts {
    margin-top: 6px;
  }
  .blocked {
    opacity: 0.75;
  }
</style>
