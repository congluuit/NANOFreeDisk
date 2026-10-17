# DevCache → AI leftovers (PLAN §13.2 C10, docs/AI_LEFTOVERS.md). Every key must
# exist in vi/ai-leftovers.ftl too.

dev-tab-ai = AI leftovers
ail-intro = What AI coding agents left behind per session: worktrees, scratchpads, browser snapshots, screenshots, test output, old Playwright browsers. Conversations are always kept. Leftovers of sessions still open in the Claude app or active in the last { $days } days are listed but not pre-selected.
ail-rescan = Scan again
ail-rescan-hint = Looks for leftovers again (read-only: nothing is deleted).
ail-finished = Clean finished sessions (safe only) · { $size }
ail-finished-hint = Selects only 🟢 leftovers of sessions that are finished, then asks before anything happens. Folders go to the quarantine (restorable for 7 days); worktrees are removed by git, their branches stay.
ail-loading = Looking for AI leftovers… (read-only)
ail-summary = { $sessions } sessions, { $projects } project folders · { $size } could be cleaned.
ail-no-git = git was not found: worktrees and common folder names (test-results, screenshots…) are not offered.
ail-no-project = Temp & tool stores
ail-session = From session “{ $title }”, active { $days ->
        [0] today
        [one] 1 day ago
       *[other] { $days } days ago
    }
ail-session-untitled = { $tool } session { $id }
ail-open = open
ail-open-hint = The session is still open (not archived) in the Claude app and may be continued: what it uses is not pre-selected.
ail-wt-orphan = Not a worktree git knows any more: the files in it are not in git.
ail-wt-prunable = The folder is gone; only git's record of it is left (git worktree prune).
ail-wt-merged = merged into { $branch }
ail-wt-pushed = pushed
ail-wt-unmerged = not merged or pushed
ail-wt-detached = (detached)
ail-wt-changes = { $count ->
        [0] no changes
        [one] 1 change
       *[other] { $count } changes
    }
ail-wt-ignored = { $size } ignored by git (to the quarantine first)
ail-wt-artifacts = It cannot be removed, but its build output can be cleaned:
ail-keep-session-open = Kept: its session is still open in the Claude app.
ail-keep-session-recent = Kept: its session was active in the last { $days } days.
ail-keep-recent-write = Kept: written in the last { $days } days.
ail-kept = { $what } ({ $tool }): conversation history, always kept.
ail-none = No AI leftovers found.
ail-clean = Clean selected
ail-clean-hint = Moves the ticked folders to the quarantine and removes the ticked worktrees with git — after a confirmation that shows exactly what runs.
ail-confirm-title = Clean AI leftovers?
ail-confirm-body = { $count ->
        [one] 1 folder
       *[other] { $count } folders
    } go to the quarantine; { $size } in total is freed.
ail-confirm-git = Then git runs { $count ->
        [one] this command
       *[other] these commands
    } (each worktree is checked again right before):
ail-confirm-note = Folders can be restored from Activity for 7 days. A removed worktree is added back with “git worktree add” on its branch, which is never deleted. Conversations are never touched.

reason-tracked = git tracks files in it.
reason-contains-history = Holds AI conversation history, which is never deleted.
reason-too-large = Too many files to check completely.
reason-no-git = Needs git, which was not found.
reason-git-failed = git could not answer for it.
reason-dirty = Has uncommitted changes or untracked files.
reason-not-merged = Its branch is neither merged into the default branch nor pushed.
reason-locked = The worktree is locked.
reason-too-many-ignored = Too many ignored files to list completely.
reason-foreign-worktree = A worktree of another repository.
reason-ignored-left = Some ignored files could not be moved to the quarantine, so git did not remove it.
reason-not-a-worktree = git no longer lists it as a worktree.

activity-kind-ai-leftovers = AI leftovers cleaned
activity-kind-worktree = Worktrees removed (git)

health-ai-leftovers = { $size } left behind by AI agents in finished sessions.
health-fix-devcache = See AI leftovers

home-ai-title = AI leftovers
home-ai-body = { $size } from finished AI sessions can be cleaned safely.
home-ai-open = Review
