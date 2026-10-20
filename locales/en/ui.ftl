# Layout shared by every view (stat cards, panels, the Explorer and the
# Recommendations of Analyze).

## Stat cards

stat-free = Free right now
stat-of = of { $total }
stat-safe = Safe to remove
stat-review = Needs review
stat-items = { $count ->
    [one] 1 item
   *[other] { $count } items
}
stat-checking = checking…
stat-outside = Not in this scan
stat-outside-unit = system & other folders
stat-outside-hint = Used space of the drive that this scan does not cover: Windows' own files it cannot read, and folders outside the one scanned.

stat-free-on = Free on { $drive }
stat-safe-scan = from the last scan
stat-safe-junk = caches & temp files
stat-last-scan = Last scan
stat-no-scan = no scan yet
stat-drives = Drives
stat-drives-low = { $count ->
    [one] 1 is low on space
   *[other] { $count } are low on space
}
stat-drives-ok = all have room

## Home

home-free-btn = Free { $size } safely
home-drives-title = Drives
home-drives-sub = Pick one to see what fills it.

## Analyze

analyze-title = Disk map · { $name }
explorer-title = Explorer
explorer-sub = What fills { $name }, sized by space used
explorer-lens-map = Map
explorer-lens-folders = Folders
explorer-mapped = { $size } mapped
explorer-color-type = File type
explorer-color-cleanup = Cleanup
legend-safe = Safe to clear
explorer-idle = Click a tile or a row to select it · double-click to open a folder
details-show = Show details
details-hide = Hide details

## Recommendations

recs-title = Recommendations
recs-sub = Tick items to clean them. Nothing is deleted before you confirm.
recs-clean-n = Clean { $count } selected
recs-details = Details
recs-copy = Copy list
recs-copy-hint = Copies the ticked items (name, size, location) as text. Nothing is deleted.
recs-copied = { $count ->
    [one] 1 item copied
   *[other] { $count } items copied
}

## Selection footer

sel-compact = { $size } selected
sel-compact-items = { $count ->
    [one] 1 item
   *[other] { $count } items
}
sel-none-short = Nothing selected
sel-clear-short = Clear
sel-select-safe = Select safe
