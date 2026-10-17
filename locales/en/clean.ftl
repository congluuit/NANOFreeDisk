# Clean view, Activity log and the safety engine (Phase 3). Loaded together with
# app.ftl; every key must exist in vi/clean.ftl too.

nav-activity = Cleanup history

## Clean
clean-title = Clean
clean-intro = Junk and caches that apps rebuild by themselves, found by the rule packs. Nothing is deleted outright: items go to the quarantine and can be restored from Activity for 7 days.
clean-activity-hint = Everything cleaned or deleted, with Restore
clean-rescan = Look again
clean-scanning = Looking for junk… (read-only)
clean-not-scanned = Not checked yet.
clean-nothing = Nothing to clean right now.
clean-checked-at = Checked { $date }
clean-level-safe = Safe
clean-level-caution = Caution
clean-level-advanced = Advanced
clean-level-protected = Protected
clean-safe-button = Clean safely ({ $size })
clean-safe-hint = If you click: every Safe item moves to the quarantine. Apps rebuild them; you can restore them from Activity for 7 days.
clean-pick-hint = Include this rule
clean-if-deleted = If deleted:
clean-close-first = { $apps } is running — close it first, or the files it keeps open are skipped.
clean-files = { $count } files
clean-preview = Preview
clean-hide = Hide
clean-preview-more = Showing the { $shown } largest of { $count } items.
clean-guide-title = Guidance only ({ $count }) — the app never deletes these
clean-selected = { $count } rule(s) selected · { $size }
clean-run = Clean selected
clean-run-hint = If you click: you see the total first, then the selected items move to the quarantine.
clean-confirm-title = Clean these items?
clean-confirm-body = { $size } from { $rules } rule(s) will move to the quarantine.
clean-confirm-note = Every item is checked again right before it moves. You can restore everything from Activity for 7 days; after that the quarantine is emptied.
clean-confirm-permanent-note = Permanent delete: nothing can be restored afterwards.
clean-confirm-running = Some apps are still running; the files they keep open stay where they are.
clean-permanent = Delete permanently instead (Expert, Safe items only)
clean-type-delete = Type DELETE to confirm:
clean-confirm-go = Move to quarantine
clean-confirm-permanent = Delete permanently
clean-running = Cleaning… { $done } / { $total }
clean-freed-so-far = { $size } freed so far
clean-cancel = Stop
clean-done-title = Freed { $size }
clean-done-detail = { $done } done, { $partial } partly (files in use), { $skipped } skipped, { $failed } failed.
clean-was-cancelled = stopped early
clean-problems = { $count } item(s) were not cleaned completely
clean-undo = Undo in Activity
clean-dismiss = Dismiss
clean-result = Freed { $size } · { $done } item(s) cleaned, { $skipped } skipped.
clean-journal-failed = Cleaned, but the undo journal could not be written ({ $error }). Quarantined items are listed in each manifest.jsonl.
clean-status-done = done
clean-status-partial = partly — some files were in use
clean-status-skipped = skipped
clean-status-failed = failed
clean-status-declined = declined
clean-status-cancelled = not done (stopped)
clean-status-not-selected = not selected

## Why an item was skipped (SafetyGate codes; the Phase 2 ones live in app.ftl)
reason-stream = Alternate data streams are never deleted.
reason-device = Device paths and reserved names (NUL, COM1…) are refused.
reason-cross-volume = A link in the path leads onto another drive.
reason-system-file = A system file where Windows or programs keep their files.
reason-system-binary = A program file inside the Windows folder.
reason-cloud-file = A cloud placeholder: deleting it would delete the cloud copy too.
reason-changed = It changed since it was checked; skipped to be safe.
reason-protected-level = The rule marks this location as protected.
reason-nested = Already included through its parent folder.
reason-in-use = Some files are in use by a running program.
reason-notfound = It is already gone.
reason-permissiondenied = Access denied (it may need administrator rights).
reason-unknown = Not deleted ({ $code }).

## Activity log
activity-title = Activity
activity-intro = Everything cleaned or deleted in this app, by day. Items in the quarantine can be restored for 7 days, items in the Recycle Bin until it is emptied.
activity-empty = Nothing cleaned or deleted yet.
activity-kind-clean = Clean
activity-kind-manual = Moved to the Recycle Bin
activity-kind-tidy = Tidy up
activity-items = { $count } item(s)
activity-freed = { $size } freed
activity-state-active = kept
activity-state-restored = restored
activity-state-partly-restored = partly restored
activity-state-purged = quarantine emptied
activity-details = Details
activity-restore = Restore ({ $count })
activity-restore-hint = If you click: the items go back where they were, with their original dates. Nothing that exists there now is overwritten.
activity-restore-one = Restore this item
activity-purge-hint = Empty this quarantine now (cannot be undone)
activity-purge-confirm = Delete { $size } from the quarantine for good? This cannot be undone.
activity-purged = Quarantine emptied.
activity-restored = Restored { $done } item(s){ $failed ->
    [0] .
   *[other] ; { $failed } could not be restored.
}
activity-restore-failed = could not be restored
activity-restore-conflicts = something with the same name is there again; kept in the quarantine
activity-restore-missing = no longer in the quarantine or the Recycle Bin
activity-restore-not-restorable = deleted permanently
activity-item-restored = restored
activity-mode-recycle-bin = Recycle Bin
activity-mode-quarantine = quarantine
activity-mode-permanent = permanent

## Home and Health Score
home-free-checking = Looking for junk…
home-free-safely = Free { $size } safely now
home-free-nothing = No junk to clean
home-free-sub = Caches and temp files apps rebuild by themselves — restorable for 7 days.
home-free-hint = If you click: the Clean view opens and asks before anything moves.
health-junk = { $size } of junk could be cleaned safely.
health-fix-clean = Clean

## Tidy up and the delete dialog
tidy-quarantine-note = Picked items move to the quarantine and can be restored from Activity for 7 days.
tidy-selected = { $count } selected
tidy-quarantine = Move to quarantine ({ $count })
tidy-quarantine-hint = If you click: you confirm first; the items move to the quarantine (restorable for 7 days).
delete-title-quarantine = Move to the quarantine?
delete-intro-quarantine = These items will move to the quarantine:
delete-quarantine-note = Restorable from Activity for 7 days. Each item is checked again right before it moves.
delete-confirm-quarantine = Move to quarantine
