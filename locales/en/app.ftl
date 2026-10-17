# NANOFreeDisk desktop app — English.
# Every key here must exist in locales/vi/app.ftl too (checked by scripts/check-locales.mjs).

app-name = NANOFreeDisk
app-tagline = See what fills your disk. Free it safely.

## Navigation

nav-home = Home
nav-analyze = Analyze
nav-clean = Junk files
nav-group-free = Free up space
nav-group-pc = PC health
nav-devcache = Dev caches
nav-duplicates = Duplicates
nav-monitor = Disk activity
nav-health = Drive health
nav-settings = Settings
nav-soon = Coming in { $phase }

## Home

home-title = Your drives
home-subtitle = Pick a drive or folder to see what uses the space. Nothing is deleted without your confirmation.
home-scan-folder = Scan a folder…
home-pick-folder = Choose a folder to analyze
home-action-analyze = See what fills the disk
home-action-analyze-hint = Treemap, sunburst and lists of the biggest folders and files.
home-action-health = Is my drive healthy?
home-action-health-hint = Good / Caution / Bad for every drive, temperature, wear and volume checks.
home-refresh = Refresh
home-no-drives = No drives found.

drive-used = { $used } used of { $total }
drive-free = { $free } free
drive-system = System
drive-not-ready = No media
drive-scan = Analyze
drive-kind-fixed = Local disk
drive-kind-removable = Removable
drive-kind-network = Network
drive-kind-optical = Optical
drive-kind-ramdisk = RAM disk
drive-kind-unknown = Drive

## Scanning

scan-title = Scanning { $path }
scan-files = { $count } files
scan-dirs = { $count } folders
scan-size = { $size } found
scan-errors = { $count } unreadable
scan-elapsed = { $time }
stat-files = Files
stat-dirs = Folders
stat-size = Size on disk
stat-time = Elapsed
scan-cancel = Cancel
scan-cancelled = Scan cancelled.
scan-failed = Scan failed: { $error }
scan-hint = Links, junctions and other volumes are never followed; hard-linked files are counted once.

## Analyze

analyze-empty = No scan yet. Analyze a drive or folder from Home.
analyze-up = Up one level
analyze-rescan = Rescan
analyze-export = Export
analyze-export-json = Report (JSON)
analyze-export-csv = All entries (CSV)
analyze-exported = Saved { $path }
analyze-summary = { $size } in { $files } files · { $dirs } folders · scanned in { $time }
analyze-issues = { $count ->
    [one] 1 path could not be read
   *[other] { $count } paths could not be read
}
analyze-of-drive = { $percent } of the drive
analyze-search-placeholder = Search names (e.g. *.iso, node_modules)…
analyze-view-treemap = Treemap
analyze-view-sunburst = Sunburst
analyze-tab-top = Largest files
analyze-tab-ext = File types
analyze-tab-search = Search results
analyze-tab-issues = Unreadable
analyze-truncated = Showing the largest items only; zoom in for more detail.
analyze-small-items = Small items
analyze-legend-folder = Folder

col-name = Name
col-size = Size on disk
col-logical = Size
col-share = Share
col-files = Files
col-modified = Modified
col-path = Path
col-type = Type
col-reason = Reason

kind-file = File
kind-dir = Folder
kind-symlink = Symbolic link
kind-junction = Junction
kind-mount_point = Mount point
kind-other = Other

attr-system = System
attr-hidden = Hidden
attr-reparse = Reparse point
attr-compressed = Compressed
attr-sparse = Sparse
attr-offline = Cloud-only
attr-readonly = Read-only
attr-hardlink = Hard link (counted once)

cat-video = Video
cat-image = Images
cat-audio = Audio
cat-archive = Archives & installers
cat-code = Code & text
cat-binary = Programs & libraries
cat-document = Documents
cat-data = Data, logs & disk images
cat-other = Other

ext-none = (no extension)

## Filters

filter-title = Filters
filter-kind = Show
filter-kind-any = Files & folders
filter-kind-files = Files only
filter-kind-dirs = Folders only
filter-min-size = Min size
filter-max-size = Max size
filter-exts = Extensions
filter-exts-placeholder = mp4, iso, zip
filter-modified = Modified
filter-modified-any = Any time
filter-modified-older-1y = Older than 1 year
filter-modified-older-3y = Older than 3 years
filter-modified-last-30d = Last 30 days
filter-hide-hidden = Hide hidden & system
filter-cloud-only = Cloud-only files
filter-apply = Search
filter-clear = Clear
filter-results = { $count } matches · { $size }
filter-invalid-size = Use sizes like 500MB or 2GB.

## Details & actions

details-none = Select an item to see details.
details-path = Location
details-contains = Contains
details-share = Share of scan
details-attrs = Attributes
action-open = Open
action-reveal = Show in Explorer
action-zoom = Zoom in
action-copy-path = Copy path
action-recycle = Move to Recycle Bin
action-recycle-hint = If you do this: the item moves to the Recycle Bin and can be restored from there. Protected Windows, program and credential locations are refused.
action-recycle-n = Move { $count } items to Recycle Bin
action-select-hint = Ctrl+click to select several items.
copied = Copied to clipboard

## Delete dialog

delete-title = Move to Recycle Bin?
delete-intro = These items will be moved to the Recycle Bin. You can restore them from there until you empty it.
delete-total = { $size } in { $files } files
delete-blocked-title = Will not be deleted
delete-extra = This is a lot of data ({ $size }, { $percent } of the drive). I have checked the list and want to continue.
delete-confirm = Move to Recycle Bin
delete-cancel = Cancel
delete-nothing = Nothing here can be deleted.
delete-nuke-note = If an item is too large for the Recycle Bin, Windows will ask before deleting it permanently.
delete-working = Moving to the Recycle Bin…
delete-result = { $done } moved ({ $size }) · { $failed } not moved
delete-declined = Kept (permanent delete declined)

reason-volume-root = The root of a drive cannot be deleted.
reason-system-dir = Inside the Windows folder — junk there is cleaned from the Clean view or with Windows tools.
reason-protected = Protected location ({ $template }): Windows, programs or credentials need it.
reason-contains-protected = Contains a protected folder ({ $template }); delete items inside it instead.
reason-link = Links, junctions and mount points are removed in Explorer, not here.
reason-contains-mount-point = Contains another drive mounted as a folder.
reason-unresolvable = The item no longer exists or cannot be accessed.

## Settings

settings-title = Settings
settings-language = Language
settings-language-system = Same as Windows
settings-theme = Theme
settings-theme-light = Light
settings-theme-dark = Dark
settings-theme-system = System
settings-theme-custom = Custom
settings-theme-system-hint = Follows Windows light/dark mode and accent color.
settings-preset = Start from preset
settings-custom-colors = Colors
settings-contrast-warning = Low contrast: { $pair } is { $ratio }:1 (WCAG AA needs 4.5:1).
settings-theme-import = Import theme.toml
settings-theme-export = Export theme.toml
settings-theme-reset = Reset colors
settings-theme-imported = Theme imported.
settings-theme-invalid = Not a valid theme file.
settings-font-scale = Text size
settings-about = About
settings-about-text = NANOFreeDisk { $version } · open source (MIT) · no telemetry.
settings-author = Made by Lưu Đức Thành Công (NANO)

token-bg = Background
token-surface = Surface
token-surface-2 = Raised surface
token-border = Borders
token-text = Text
token-text-muted = Secondary text
token-accent = Accent
token-accent-contrast = Text on accent
token-danger = Danger
token-risk-safe = Risk: safe
token-risk-caution = Risk: caution
token-risk-advanced = Risk: advanced
token-risk-protected = Risk: protected
token-tm-1 = Treemap: video
token-tm-2 = Treemap: images
token-tm-3 = Treemap: audio
token-tm-4 = Treemap: archives
token-tm-5 = Treemap: code
token-tm-6 = Treemap: programs
token-tm-7 = Treemap: documents
token-tm-8 = Treemap: data
token-tm-other = Treemap: other
token-tm-dir = Treemap: folder frame

## Command palette (Ctrl+K)

palette-open = Search everything…
palette-title = Command palette (Ctrl+K)
palette-placeholder = Type a command, drive, folder or setting…
palette-nothing = Nothing matches.
palette-move = move
palette-run = run
palette-folders-hint = Type 2+ letters to find folders in this scan.
palette-group-go = Go to
palette-group-action = Actions
palette-group-drive = Drives
palette-group-folder = Folders in this scan
palette-group-setting = Settings
palette-mode-simple = Switch to Simple mode
palette-mode-expert = Switch to Expert mode
palette-scan-drive = Analyze { $drive }
palette-font-larger = Larger text
palette-font-smaller = Smaller text

## Simple / Expert mode

mode-label = Mode
mode-simple = Simple
mode-expert = Expert
mode-simple-hint = Only what most people need: clear lists and safe actions.
mode-expert-hint = Everything: advanced filters, attributes, unreadable paths, JSON/CSV export and the equivalent nfd command.
cli-hint = Command:
cli-hint-title = The nfd command line does the same thing (for scripts and Task Scheduler).
action-copy-command = Copy command

## Old & large

nav-oldlarge = Old & large
ol-title = Old & large files
ol-intro = The quickest wins: very large files, big files nobody changed for a long time, and old installers left in Downloads. Pick what you no longer need — it goes to the Recycle Bin after a safety check.
ol-only-folder = Only the folder open in Analyze
ol-large-min = Large means at least
ol-stale-age = Old means not modified for
ol-stale-min = Old files from
ol-installer-age = Installers older than
ol-months = { $count } months
ol-years = { $count ->
    [one] 1 year
   *[other] { $count } years
}
ol-days = { $count } days
ol-summary = { $files ->
    [one] 1 file
   *[other] { $files } files
    } · { $size } in total
ol-large-title = Large files
ol-large-hint = { $size } or bigger
ol-stale-title = Not modified for long
ol-stale-hint = Unchanged for { $age }
ol-installers-title = Old installers
ol-installers-hint = .exe .msi .iso .zip… in Downloads, older than { $days } days
ol-files = { $count ->
    [one] 1 file
   *[other] { $count } files
}
ol-selected = { $count } selected · { $size }
ol-select-all = Select all
ol-stale-note = Windows does not reliably record when a file was last opened, so "old" means not modified.
ol-more = Showing the { $shown } largest of { $count }.
ol-none = Nothing here — nice and tidy.

## Tidy up (empty folders, broken shortcuts)

nav-tidy = Tidy up
tidy-title = Tidy up
tidy-intro = Empty folders and shortcuts whose target is gone. They take almost no space but clutter Explorer and the Start menu.
tidy-empty-title = Empty folders
tidy-empty-summary = { $count ->
    [one] 1 empty folder tree
   *[other] { $count } empty folder trees
    } ({ $total } folders in all). Folders that could not be read are never listed.
tidy-empty-none = No empty folders.
tidy-empty-self = Nothing
tidy-nested = { $count ->
    [one] 1 empty folder
   *[other] { $count } empty folders
}
tidy-links-title = Broken shortcuts
tidy-links-checking = Checking shortcuts…
tidy-links-summary = { $checked } shortcuts checked; { $unchecked } point to network shares or special locations and were not checked.
tidy-links-none = No broken shortcuts.
tidy-target = Target
tidy-status-ok = Works
tidy-status-missing = Target missing
tidy-status-drive_missing = Drive not connected
tidy-status-unchecked = Not checked

## HTML report

analyze-export-html = Report (HTML)
report-title = Disk report
report-generated = Generated { $date } · NANOFreeDisk { $version }
report-treemap = What fills the space
report-children = Largest items
report-changes = What changed since { $date }
report-drive-share = Share of the drive
report-footer = Created by NANOFreeDisk { $version } — read-only analysis, nothing was changed or deleted. No telemetry.

## What changed? (scan history)

analyze-tab-changes = What changed?
changes-history-off = Scan history is off (Settings → Scan history), so there is nothing to compare with.
changes-not-recorded = This scan was not saved to the history, so it cannot be compared.
changes-first = This is the first saved scan of this folder. Scan it again in a few days to see what changed.
changes-compare = Compare with the scan of
changes-summary = { $days ->
        [0] Since then
        [one] In 1 day
       *[other] In { $days } days
    }: { $size } · { $files } files
changes-free = Free space on the drive { $size }
changes-none = No folder or large file changed by more than 10 MB.
changes-grew = Grew
changes-shrank = Shrank
changes-files-up = Large files added or grown
changes-files-down = Large files removed or shrunk
changes-new = new
changes-gone = gone
changes-footnote = Each folder is shown at the most specific level that explains the change. Small folders and files are not kept in the history, so tiny changes are left out.
history-save-failed = The scan worked, but it could not be saved to the history: { $error }

## Health score

health-title = Health score
health-good = Looks good
health-fair = Needs attention
health-poor = Act soon
health-all-good = Nothing to fix right now.
health-basis = Based on free space, how fast each drive fills (from your scan history), the last scan and each drive's own health (S.M.A.R.T. / NVMe, temperature).
health-low-space = { $drive } has only { $percent } free ({ $size }).
health-filling-fast = At the current pace { $drive } will be full in about { $days } days.
health-old-large = { $size } of large or old files in { $folder } could be freed.
health-no-scan = Analyze a drive so the score can include what fills it.
health-no-trend = Scan { $drive } again on another day to see how fast it fills up.
health-fix-oldlarge = Free space
health-fix-analyze = See what grew
health-fix-home = Analyze now

settings-history = Scan history
settings-history-keep = Keep a history of scans
settings-history-hint = Stores folder sizes and large file names (never file contents) of the last 30 scans of each folder on this PC only. Used by "What changed?" and the health score.
settings-history-count = { $count ->
        [one] 1 saved scan
       *[other] { $count } saved scans
    }
settings-history-clear = Clear history
settings-history-clear-confirm = Delete every saved scan? "What changed?" will start over from the next scan.
settings-history-cleared = Scan history cleared.

## Folder Encyclopedia

encyclopedia-title = What is this?
encyclopedia-inside = Inside:
encyclopedia-note = From the Folder Encyclopedia (rules/encyclopedia). Information only — NANOFreeDisk never deletes anything because of it.
risk-safe = Safe to remove
risk-caution = Remove with care
risk-advanced = Experts only
risk-protected = Do not delete

## Age heatmap, owner, broken links

analyze-tab-age = Age
age-hint = Space by when files were last modified. Darker cells hold more bytes; click a folder to open it.
age-col-0 = < 1 month
age-col-1 = 1–6 months
age-col-2 = 6–12 months
age-col-3 = 1–2 years
age-col-4 = 2–5 years
age-col-5 = > 5 years
age-col-6 = No date
age-direct = Files directly here
age-other = Everything else
age-total = Total
details-owner = Owner
details-owner-hint = The account that owns this item (from its Windows security settings).
tidy-junctions-title = Broken junctions and symbolic links
tidy-junctions-summary = { $checked } links checked. A link whose target is gone shows an error when opened.
tidy-junctions-none = No broken junctions or symbolic links.

## "If you do this…" hints (G4)

hint-scan = If you do this: the folder is read (nothing is changed) and, if scan history is on, a summary of sizes is saved on this PC.
hint-rescan = If you do this: the folder is read again and these results are replaced; with scan history on, a new snapshot is saved.
hint-cancel-scan = If you do this: the scan stops and its partial results are discarded. Nothing on disk changes.
hint-export = If you do this: a new file is written where you choose; nothing else changes.
hint-theme = If you do this: the colors change right away; switch back at any time.
hint-theme-import = If you do this: your custom colors are replaced by the file's. Export first to keep them.
hint-theme-reset = If you do this: your custom color changes are dropped and the preset colors come back.
hint-theme-preset = If you do this: the theme switches to Custom based on this preset; your previous custom colors are replaced.
hint-mode = If you do this: only what is shown changes; no setting or file is touched.
hint-language = If you do this: the app switches language immediately.
hint-font = If you do this: the text size changes right away.
hint-history-toggle = If you turn this off: new scans are no longer saved; saved ones stay until you clear them.
hint-history-clear = If you do this: every saved scan is deleted from this PC (you are asked first). Your files are not touched.

## Explorer integration (A7)

shell-menu-caption = Analyze with NANOFreeDisk
settings-shell = Explorer integration
settings-shell-toggle = Show "Analyze with NANOFreeDisk" when right-clicking a folder or drive
settings-shell-hint = Added for your account only (no administrator rights). Turning it off, or uninstalling NANOFreeDisk, removes it again. If the app is already open, the folder opens in that window.
settings-shell-on = Explorer menu added.
settings-shell-off = Explorer menu removed.
hint-shell = If you do this: an entry is added to (or removed from) Explorer's right-click menu for your account only; nothing else changes.

## Errors

error-generic = Something went wrong: { $message }
error-no-session = Scan a drive or folder first.
