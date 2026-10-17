//! Git worktrees of AI sessions (`docs/AI_LEFTOVERS.md` §2.4). A worktree is never
//! deleted as a raw folder: git is asked what it is (`worktree list --porcelain`),
//! whether anything in it would be lost (`status`, merged into the default branch or
//! pushed), and removes it itself (`worktree remove`, then `worktree prune`). Its
//! ignored files — build output, but also `.env` files — go to the quarantine first
//! through the executor, so nothing git would delete for good is lost. Branches are
//! never deleted.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use nfd_core::exec::{ItemResult, ItemStatus, Operation};
use nfd_core::plan::DeleteMode;
use serde::Serialize;

use crate::norm;
use crate::process::{self, RunOutput};

/// Longest a git query may run.
const QUERY: Duration = Duration::from_secs(60);
/// Longest `git worktree remove` may run (it deletes the tracked files).
const REMOVE: Duration = Duration::from_secs(600);
/// Bytes of a git listing kept (more = the listing is incomplete).
const LISTING: usize = 8 << 20;

/// Comparable form of a path the way git reports it: links resolved (`/var` →
/// `/private/var` on macOS) and short 8.3 names made long (`RUNNER~1`), so a
/// worktree is recognised whatever spelling the scan used. A path that is gone is
/// resolved through its parent.
#[must_use]
pub fn real_key(path: &Path) -> String {
    let real = std::fs::canonicalize(path).ok().or_else(|| {
        let parent = std::fs::canonicalize(path.parent()?).ok()?;
        Some(parent.join(path.file_name()?))
    });
    let text = real.map_or_else(
        || path.to_string_lossy().into_owned(),
        |r| r.to_string_lossy().into_owned(),
    );
    let text = text
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| text.strip_prefix(r"\\?\").map(str::to_owned))
        .unwrap_or(text);
    norm(&text)
}

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WtEntry {
    pub path: PathBuf,
    pub head: Option<String>,
    /// Short branch name (`cong/brave-fox`).
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// Parse `git worktree list --porcelain` (blocks separated by empty lines).
#[must_use]
pub fn parse_porcelain(text: &str) -> Vec<WtEntry> {
    let mut out = Vec::new();
    let mut cur: Option<WtEntry> = None;
    for line in text.lines().map(|l| l.trim_end_matches('\r')) {
        if line.is_empty() {
            out.extend(cur.take());
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key == "worktree" {
            out.extend(cur.take());
            cur = Some(WtEntry {
                path: PathBuf::from(value),
                ..WtEntry::default()
            });
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        match key {
            "HEAD" => e.head = Some(value.to_owned()),
            "branch" => {
                e.branch = Some(
                    value
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value)
                        .to_owned(),
                );
            }
            "detached" => e.detached = true,
            "bare" => e.bare = true,
            "locked" => e.locked = true,
            "prunable" => e.prunable = true,
            _ => {}
        }
    }
    out.extend(cur);
    out
}

fn args(dir: &Path, rest: &[&str]) -> Vec<String> {
    let mut all = vec![
        "-c".to_owned(),
        "core.quotepath=off".to_owned(),
        "-C".to_owned(),
        dir.to_string_lossy().into_owned(),
    ];
    all.extend(rest.iter().map(|a| (*a).to_owned()));
    all
}

fn git_run(git: &Path, dir: &Path, rest: &[&str], timeout: Duration) -> Option<RunOutput> {
    let out = process::run_keeping(git, &args(dir, rest), None, timeout, None, LISTING).ok()?;
    (!out.timed_out && out.code.is_some()).then_some(out)
}

/// `true` when git exits with 0.
fn git_ok(git: &Path, dir: &Path, rest: &[&str]) -> bool {
    git_run(git, dir, rest, QUERY).is_some_and(|o| o.code == Some(0))
}

/// NUL-separated output of a successful git command; `None` when it failed or the
/// listing did not fit.
fn git_z(git: &Path, dir: &Path, rest: &[&str]) -> Option<Vec<String>> {
    let out = git_run(git, dir, rest, QUERY)?;
    if out.code != Some(0) || out.truncated {
        return None;
    }
    Some(
        out.stdout
            .split('\0')
            .map(|s| s.trim_matches(['\r', '\n']))
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

/// The worktrees git knows for `repo`.
#[must_use]
pub fn list(git: &Path, repo: &Path) -> Option<Vec<WtEntry>> {
    let out = git_run(git, repo, &["worktree", "list", "--porcelain"], QUERY)?;
    (out.code == Some(0) && !out.truncated).then(|| parse_porcelain(&out.stdout))
}

/// The default branch of `repo`: `origin/HEAD`, else `main` / `master`.
#[must_use]
pub fn default_branch(git: &Path, repo: &Path) -> Option<String> {
    if let Some(out) = git_run(
        git,
        repo,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
        QUERY,
    )
    .filter(|o| o.code == Some(0))
    {
        if let Some(b) = out.stdout.trim().strip_prefix("refs/remotes/origin/") {
            return Some(b.to_owned());
        }
    }
    ["main", "master"].into_iter().map(str::to_owned).find(|b| {
        git_ok(
            git,
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{b}"),
            ],
        )
    })
}

/// What git says about one worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitState {
    /// Uncommitted changes and untracked (not ignored) files.
    pub changes: u32,
    pub default_branch: Option<String>,
    /// `HEAD` is contained in the default branch (local or `origin/`).
    pub merged: bool,
    /// `HEAD` is contained in a remote-tracking branch.
    pub pushed: bool,
    /// What git ignores, relative to the worktree (`/`-separated, folders without a
    /// trailing slash). `None` = could not be listed completely.
    pub ignored: Option<Vec<String>>,
}

/// Ask git about the worktree `entry` of `repo`. `None` when git cannot answer.
#[must_use]
pub fn state(git: &Path, repo: &Path, entry: &WtEntry) -> Option<GitState> {
    let wt = &entry.path;
    let status = git_z(
        git,
        wt,
        &[
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?;
    let head = entry.head.clone()?;
    let default = default_branch(git, repo);
    let merged = default.as_ref().is_some_and(|b| {
        [
            format!("refs/heads/{b}"),
            format!("refs/remotes/origin/{b}"),
        ]
        .iter()
        .any(|r| git_ok(git, repo, &["merge-base", "--is-ancestor", &head, r]))
    });
    let pushed = git_z(
        git,
        repo,
        &[
            "for-each-ref",
            "--contains",
            &head,
            "--format=%(refname)%00",
            "refs/remotes",
        ],
    )
    .is_some_and(|r| r.iter().any(|x| !x.ends_with("/HEAD")));
    let ignored = git_z(
        git,
        wt,
        &[
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ],
    )
    .map(|l| {
        l.into_iter()
            .map(|p| p.trim_end_matches('/').to_owned())
            .collect()
    });
    Some(GitState {
        changes: u32::try_from(status.len()).unwrap_or(u32::MAX),
        default_branch: default,
        merged,
        pushed,
        ignored,
    })
}

/// A worktree the user chose to remove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeJob {
    pub repo: PathBuf,
    pub path: PathBuf,
    pub branch: Option<String>,
    /// The commit seen when it was planned (anything else = changed since).
    pub head: Option<String>,
    /// The folder is already gone: only `git worktree prune`.
    pub prune_only: bool,
    /// Bytes git deletes (the tracked files; ignored ones are quarantined first).
    pub bytes: u64,
    pub files: u64,
}

impl WorktreeJob {
    /// What runs, as shown before the user confirms.
    #[must_use]
    pub fn command_line(&self) -> String {
        let repo = self.repo.display();
        if self.prune_only {
            format!("git -C \"{repo}\" worktree prune")
        } else {
            format!(
                "git -C \"{repo}\" worktree remove \"{}\" && git -C \"{repo}\" worktree prune",
                self.path.display()
            )
        }
    }

    /// How to get the worktree back (the branch is never deleted).
    #[must_use]
    pub fn undo_hint(&self) -> Option<String> {
        let branch = self.branch.as_ref()?;
        Some(format!(
            "git -C \"{}\" worktree add \"{}\" {branch}",
            self.repo.display(),
            self.path.display()
        ))
    }
}

fn result(job: &WorktreeJob, n: usize, status: ItemStatus, reason: Option<&str>) -> ItemResult {
    let done = status == ItemStatus::Done;
    let mut message = job.command_line();
    if let Some(u) = job.undo_hint().filter(|_| done && !job.prune_only) {
        message = format!("{message} · undo: {u}");
    }
    ItemResult {
        n,
        path: job.path.clone(),
        rule_id: Some(
            if job.prune_only {
                "command:git-worktree-prune"
            } else {
                "command:git-worktree-remove"
            }
            .to_owned(),
        ),
        mode: DeleteMode::Permanent,
        is_dir: true,
        bytes: job.bytes,
        files: job.files,
        freed: if done { job.bytes } else { 0 },
        status,
        reason: reason.map(str::to_owned),
        message: Some(message),
        stored: None,
        warnings: Vec::new(),
    }
}

/// Why `job` must not be removed now, checked right before git runs: still a clean
/// worktree of `repo` at the planned commit, merged or pushed, unlocked, and nothing
/// ignored left in it (whatever the quarantine could not move would be lost).
#[must_use]
pub fn recheck(git: &Path, job: &WorktreeJob) -> Option<&'static str> {
    let Some(entries) = list(git, &job.repo) else {
        return Some("git-failed");
    };
    let want = real_key(&job.path);
    let Some(entry) = entries.iter().find(|e| real_key(&e.path) == want) else {
        return Some("not-a-worktree");
    };
    if job.prune_only {
        return (!entry.prunable).then_some("changed");
    }
    if entry.locked {
        return Some("locked");
    }
    if entry.head != job.head {
        return Some("changed");
    }
    let Some(st) = state(git, &job.repo, entry) else {
        return Some("git-failed");
    };
    if st.changes > 0 {
        Some("dirty")
    } else if !st.merged && !st.pushed {
        Some("not-merged")
    } else if st.ignored.as_ref().is_none_or(|i| !i.is_empty()) {
        Some("ignored-left")
    } else {
        None
    }
}

/// Remove (or prune) one worktree with git after [`recheck`].
#[must_use]
pub fn remove(git: &Path, job: &WorktreeJob, n: usize, cancel: Option<&AtomicBool>) -> ItemResult {
    if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
        return result(job, n, ItemStatus::Cancelled, None);
    }
    if let Some(why) = recheck(git, job) {
        return result(job, n, ItemStatus::Skipped, Some(why));
    }
    if !job.prune_only {
        let path = job.path.to_string_lossy().into_owned();
        let out = git_run(git, &job.repo, &["worktree", "remove", &path], REMOVE);
        match out {
            Some(o) if o.code == Some(0) => {}
            Some(o) => {
                let mut r = result(job, n, ItemStatus::Failed, Some("git-failed"));
                let err: String = o.stderr.trim().chars().take(300).collect();
                r.message = Some(format!("{} · {err}", job.command_line()));
                return r;
            }
            None => return result(job, n, ItemStatus::Failed, Some("git-failed")),
        }
    }
    // a failed prune leaves a stale entry git cleans itself later
    let _ = git_run(git, &job.repo, &["worktree", "prune"], QUERY);
    result(job, n, ItemStatus::Done, None)
}

/// The git commands of one clean as an Activity-log operation (kind `worktree`, not
/// restorable: the message says how to add the worktree back).
#[must_use]
pub fn operation(items: Vec<ItemResult>, op_id: String, started: i64, finished: i64) -> Operation {
    Operation {
        id: op_id,
        kind: "worktree".into(),
        started_at: started,
        finished_at: finished,
        cancelled: items.iter().any(|i| i.status == ItemStatus::Cancelled),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_porcelain_listing() {
        let text = "worktree D:/r\nHEAD 8b0b\nbranch refs/heads/main\n\n\
                    worktree D:/r/.claude/worktrees/fox\nHEAD 772e\nbranch refs/heads/cong/fox\nlocked\n\n\
                    worktree D:/r/.claude/worktrees/gone\nHEAD 1234\ndetached\nprunable gitdir file points to non-existent location\n";
        let e = parse_porcelain(text);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].branch.as_deref(), Some("main"));
        assert_eq!(e[1].branch.as_deref(), Some("cong/fox"));
        assert!(e[1].locked && !e[1].prunable);
        assert!(e[2].detached && e[2].prunable);
        assert_eq!(e[2].path, PathBuf::from("D:/r/.claude/worktrees/gone"));
    }

    #[test]
    fn job_lines() {
        let job = WorktreeJob {
            repo: PathBuf::from("D:/r"),
            path: PathBuf::from("D:/r/.claude/worktrees/fox"),
            branch: Some("cong/fox".into()),
            head: Some("772e".into()),
            prune_only: false,
            bytes: 10,
            files: 1,
        };
        assert!(job.command_line().contains("worktree remove"));
        assert!(job.undo_hint().expect("hint").ends_with("cong/fox"));
    }
}
