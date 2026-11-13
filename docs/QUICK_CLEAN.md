# Quick clean

Quick clean is the one-button path for people who do not want to learn every tool:
one scan, one list, one confirmation, one result. It adds no new way to delete
anything. Each part of the scan calls the scan of the tool it belongs to, and each
part of the clean calls that tool's own run function, so every item still goes
Planner, SafetyGate, Executor and lands in the quarantine, restorable from Cleanup
history for 7 days.

Code: `apps/nanofreedisk/src/lib/quick/model.ts` (pure, tested in
`src/lib/quick.test.ts`), `src/lib/quick/state.svelte.ts` (runs the steps and the
parts), `src/lib/views/QuickClean.svelte` (the screen).

The idea follows the "Smart Scan" / "Health Check" pattern of established cleaners
(one button, a visible list of what is being checked, results grouped by what you
can do about them, one total) and BleachBit's preview-then-clean. It leaves out
their dark patterns: no alarm colors or "issues found" counters, no figure larger
than what will really be freed, nothing personal ticked for you.

## The steps

They run one after the other, in this order. Every step only reads. A step that
fails is marked "Could not finish" and the next one runs; **Stop** asks the running
step to stop (the drive scan and the duplicate search are cancelled in the backend)
and marks the rest as stopped. The result then shows what the finished steps found.

| Step | What it calls | Platform |
|---|---|---|
| Junk files | the Junk files rule scan (`scanJunk`, `clean_scan`) | Windows |
| Developer caches | the Dev caches rule scan (`scanDev`, `devcache_scan`) | all |
| Recycle Bin | `recyclebin_status` | Windows |
| Scan of the system drive | the app's scan session (`startScan`); a scan of the same drive younger than 15 minutes is used as is. Its progress shows in the step list instead of the scan window, and the user is not taken to the Disk map when it ends. The Disk map, Tidy up and Old & large use this scan too. | all |
| Empty folders and broken shortcuts | Tidy up's queries on that scan (`empty_folders`, `broken_shortcuts`) | all |
| Safe suggestions from the scan | the Disk map's advice (`advice_summary`, waiting up to 2 minutes for it to be computed) | all |
| Large and old files | Old & large's query (`old_and_large`, the thresholds from Settings) | all |
| Duplicates in your folders | Duplicates' one-click search of the personal folders (`findPersonal`); its result is what the Duplicates view shows when opened | all |

Without a system drive, the drive scan and the three steps that read it are skipped
with a note; the other steps still run.

## The three groups

**Safe to remove** (ticked when the result opens)

- Junk files: Safe rules of the junk scan.
- Developer caches: Safe rules of the Dev caches scan.
- More from the disk scan: the Disk map's suggestions that are Safe, actionable,
  pre-selected by the backend and go to the quarantine (`clean` or `quarantine`
  action), such as build output.
- Empty folders.
- Broken shortcuts whose target is gone from a drive that is there.

**Worth a look** (never ticked)

- Junk that needs a look: Caution rules. Ticking it asks for an acknowledgement in
  the confirmation, like Junk files does.
- Duplicate files and Large and old files: a summary and **Review**, which opens
  Duplicates or Old & large with the results already there.
- More suggestions: Caution suggestions of the Disk map and Safe ones Quick clean
  does not run (AI agent folders, which git removes, and Recycle Bin moves); Review
  opens the Disk map.
- Recycle Bin: its size and **Empty the Recycle Bin**, behind its own confirmation,
  because emptying it is permanent. It is never part of the Clean button.

**Good to know** (no action)

- Space that needs the proper Windows tool (Protected rules with a Windows tool,
  the Disk map's "guide" groups such as the page file), with a button that opens
  the tool.
- How much of the junk needs administrator rights. After the clean, Junk files
  offers to finish it with one Windows prompt.

Never pre-ticked, anywhere: Caution items, the Recycle Bin, duplicates, large and
old files, Advanced rules (they stay in Junk files, Expert mode), shortcuts that
point to a drive that is not connected (it may be a USB stick that is unplugged),
anything the backend did not pre-select.

## How totals are kept honest

The same bytes are never counted twice:

1. A Dev caches rule whose id the junk scan also has is counted once, in Junk files.
2. A Disk map suggestion that comes from a rule (`reason` `rule` or `keep-rule`, and
   every "junk elsewhere" group) is left out when the junk scan or the Dev caches
   scan has that rule: the rule scans walk the whole machine, so the suggestion is
   part of what they already count. A rule neither scan has (for example on macOS)
   still counts through the suggestion.
3. Empty folders and broken shortcuts that lie inside a ticked suggestion go with it
   and are not listed again.
4. Only rows that can be ticked add to the Clean button. Duplicates, large files,
   the Recycle Bin and everything in "Good to know" never do.
5. Files of a rule whose app is open (Chrome, Edge…) are taken out of the total and
   named in a "close these first" line, since the clean skips them while the app
   runs.

The total says **up to** when it is an estimate: when more than one path-based
source is ticked (junk, Caution junk, Dev caches, suggestions), because two rules or
a rule and a suggestion can match the same folder, or when open apps hold part of
it. The result screen shows what was really freed, as each tool's operation
reports it.

## The clean

One confirmation lists each part, its size and where it goes (the quarantine), what
Tidy up's planner keeps back, the apps to close first, and the acknowledgements
Junk files and Tidy up ask for (Caution items; a very large removal). The parts
then run one after the other, each through its tool's own function:

1. Suggestions from the disk scan: `cleanAdvice` (`advice_clean`, Safe groups only).
2. Empty folders and broken shortcuts: Tidy up's path, `delete_entries` with the
   quarantine, for the items `plan_delete` allowed (the SafetyGate checks each one
   again before it moves).
3. Junk files: `runClean` (`clean_run`, not permanent).
4. Dev caches: `runDev` (`devcache_run`, not permanent).

The scan's own entries go first because a rule clean would make them stale. While
the parts run, the per-tool success toasts are held back (errors still show) and
one result screen takes their place: what was freed per part, how many items stayed
and why, the junk clean's leftovers grouped by cause (in use, needs administrator
rights, other) with **Finish in Junk files** when one of them has an action, and
**See it in Cleanup history**. The last result is kept in the settings
(`lastQuick`) for Home.
