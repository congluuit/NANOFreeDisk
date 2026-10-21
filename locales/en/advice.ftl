# Analyze → Suggestions: what can be deleted, what needs care, what must stay, and
# why (nfd-advice). Loaded with app.ftl; every key must exist in vi/advice.ftl.

## Verdicts
advice-verdict-safe = Safe to delete
advice-verdict-caution = Needs care
advice-verdict-guide = Guide only
advice-verdict-keep = Better keep
advice-verdict-unknown = Unknown
advice-badge-safe = Safe
advice-badge-caution = Care
advice-badge-guide = Guide
advice-badge-keep = Keep
advice-badge-unknown = —
advice-badge-partly = { $size } can go
advice-verdict-hint-safe = Recreated automatically or not needed any more.
advice-verdict-hint-caution = May go, but something is lost (history, a re-download, a long rebuild).
advice-verdict-hint-guide = Only Windows' or the tool's own command shrinks it safely.
advice-verdict-hint-keep = The app suggests keeping it (in use, not saved elsewhere, settings…). It can be wrong: you can still Delete anyway, restorable from the quarantine.
advice-verdict-hint-unknown = Nothing known about it: only you can decide.
advice-verdict-protected = Protected
advice-badge-protected = Protected
advice-verdict-hint-protected = Hard protection: Windows, programs, credentials or AI conversation history. Never deleted, not even on request.

## Panel
advice-tab = Suggestions
advice-headline = You can free { $size }
advice-headline-none = Nothing here can be freed safely right now.
advice-headline-sub = Of { $total } scanned: everything below says what happens if it goes, and what stays.
advice-computing = Working out what can go and what must stay…
advice-checking-git = Checking AI agent worktrees with git…
advice-not-ready = The suggestions appear here as soon as they are worked out.
advice-section-safe = Safe to remove
advice-section-safe-hint = Recreated automatically or not needed: ticked for you.
advice-section-caution = Review first
advice-section-caution-hint = Something is lost (history, a re-download, a long rebuild): tick only what you are sure of.
advice-section-guide = Only with the proper tool
advice-section-guide-hint = Windows' or the tool's own command shrinks these; NANOFreeDisk shows how.
advice-section-keep = Big but keep
advice-section-keep-hint = Why each one stays. What only the app judged can still be deleted anyway; hard protections (Windows, programs, credentials, conversation history) never.
advice-section-elsewhere = Junk elsewhere on this PC
advice-section-elsewhere-hint = Found by the cleanup rules outside the scanned folder.
advice-empty-section = Nothing here.
advice-expert-only = Expert mode only
advice-col-verdict = Verdict
advice-col-reclaim = Can free
advice-color-type = Color by file type
advice-color-verdict = Color by verdict

## Why (titles)
advice-reason-rule = { $app }
advice-reason-keep-rule = { $app } (kept)
advice-reason-build-output = { $kind } build output
advice-reason-agent-worktree = AI agent worktrees
advice-reason-agent-leftover = { $app } leftovers
advice-reason-agent-history = { $app } conversation history
advice-reason-system = Windows
advice-reason-programs = Installed programs
advice-reason-protected = Protected location
advice-reason-personal = Your photos, videos and music
advice-reason-explained = { $title }
advice-reason-old-large = Old large files
advice-reason-old-installer = Old installers in Downloads
advice-reason-duplicate = Duplicate copies
advice-in-folders = { $count ->
    [one] in 1 folder
   *[other] in { $count } folders
}
advice-items-more = and { $count } more
advice-recent = { $size } written recently stays for now
advice-if-delete = If you delete it
advice-why-keep = Why it stays
advice-untouched = What is not touched
advice-close-first = Close first: { $apps }
advice-store-better = The package manager's own command does it: { $id }

## Delete anyway (the user overrules the app)
advice-override = Delete anyway…
advice-override-hint = The app suggests keeping it, but it can be wrong. You decide: it moves to the quarantine (restorable for 7 days).
advice-override-title = Delete anyway?
advice-override-body = { $count ->
    [one] NANOFreeDisk suggested keeping this item ({ $size }). You may know better.
   *[other] NANOFreeDisk suggested keeping these { $count } items ({ $size }). You may know better.
}
advice-override-why = Why the app kept it
advice-override-ways = Nothing is deleted for good: a git worktree with nothing uncommitted is removed with `git worktree remove` (its branch and commits stay); everything else moves to the quarantine and can be restored from Cleanup history for 7 days.
advice-override-ack = I understand and want to delete it anyway
advice-override-go = Delete anyway
advice-override-done = Moved { $size } out of the way ({ $count } items). Restore it from Cleanup history if needed.
advice-override-skipped = { $count } items are hard-protected and were left alone.
advice-hard-hard = Hard protection: Windows, programs, credentials or AI conversation history are never deleted, not even on request.
advice-hard-inside-hard = Hard protection: it lies inside Windows, a program folder, credentials or conversation history.
advice-hard-contains-hard = It holds something protected (conversation history, credentials…), so it cannot go as a whole: delete the other parts inside it instead.
advice-hard-root = The scanned folder itself is not deleted from here.
advice-hard-other = The SafetyGate refuses it ({ $code }).

## Why not now
advice-blocked-git-check = Waiting for git to check it…
advice-blocked-unverified = Not confirmed by git yet: check it in Dev caches → AI leftovers.
advice-blocked-session-live = Its AI session is still working (active in the last 24 hours).
advice-blocked-session-open = Its session is still open in the Claude app: archive the session there first.
advice-blocked-dirty = It has uncommitted changes.
advice-blocked-not-merged = Its branch is neither merged nor pushed.
advice-blocked-not-removable = git cannot remove it yet (uncommitted changes or an unmerged branch).
advice-blocked-newest = The newest revision stays.
advice-blocked-contains-history = Conversation history is inside.
advice-blocked-tracked = git tracks files in it.
advice-blocked-other = Not possible right now ({ $code }).

## Actions
advice-action-clean = Clean with the rule
advice-action-quarantine = Move to the quarantine
advice-action-ai-leftovers = Remove with git
advice-action-recycle-bin = Move to the Recycle Bin
advice-action-store = Run the official command
advice-action-duplicates = Review the duplicates
advice-action-guide = How to shrink it
advice-action-show = Show in the tree
advice-action-hint-clean = If you do this: the rule's own walk moves what it covers into the quarantine (7 days to restore); recent files and 🔒 kept paths stay.
advice-action-hint-quarantine = If you do this: the folder moves into the quarantine and can be restored from Activity for 7 days.
advice-action-hint-ai-leftovers = If you do this: build output goes to the quarantine, then `git worktree remove` (never --force) removes the worktree; the branch stays.
advice-action-hint-recycle-bin = If you do this: it moves to the Recycle Bin, where you can restore it.
advice-action-hint-store = Opens Dev caches → Stores, where the exact command is shown before it runs.
advice-action-hint-duplicates = Opens Duplicates to choose which copy stays.
advice-action-hint-guide = Shows the explanation of what it is and the proper tool.
advice-clean-n = Clean { $count } selected ({ $size })
advice-clean-hint = If you do this: rules and build output go to the quarantine (7 days to restore), finished agent worktrees are removed with git, personal picks go to the Recycle Bin.

## Confirmation
advice-confirm-title = Clean the selected suggestions?
advice-confirm-body = { $count ->
    [one] 1 suggestion, about { $size }.
   *[other] { $count } suggestions, about { $size }.
}
advice-confirm-ways = Rules and build output go to the quarantine (restorable for 7 days), finished agent worktrees are removed with git (the branch stays), personal picks go to the Recycle Bin.
advice-confirm-note = Every item is checked again by the SafetyGate right before it is touched. Conversation history, protected locations and 🔒 kept paths are never included.
advice-confirm-go = Clean
advice-done = Freed { $size } ({ $count } items).
advice-done-problems = { $count } items were skipped or failed: see Activity.
advice-skipped = { $count } suggestions were left out (not confirmed, Expert only or holding something kept).

## Details panel
advice-details-title = Can I delete this?
advice-details-loading = Checking…
advice-details-inside = Inside
advice-details-parts = What it holds
advice-details-part-of = Part of { $path }
advice-details-unknown = Nothing known about it: only you can decide. Deleted by hand, it goes to the Recycle Bin.
advice-details-mixed = A mix: { $free } can be freed through the suggestions, the rest stays.
advice-free-here = Free { $size } here
advice-details-nothing = Nothing inside can be freed by the app.
advice-details-open-suggestion = See in Suggestions

## Home, Health Score, Activity
home-free-advice = Free { $size } — see suggestions
home-free-advice-sub = From the last scan: what can go, what needs care and what must stay.
health-reclaimable = { $size } in { $folder } can be freed (see the suggestions).
health-fix-suggestions = See suggestions
activity-kind-advice = Suggestions (Analyze)
activity-kind-override = Deleted anyway (your decision)
