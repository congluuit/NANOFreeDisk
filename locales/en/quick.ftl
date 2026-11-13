## Quick clean, the Tools page and the Simple sidebar

nav-quick = Quick clean
nav-diskmap = Disk map
nav-tools = Tools
nav-startup-apps = Startup apps

## Tools page

tools-title = Tools
tools-intro = Every tool of the app. Quick clean runs the safe part of several of them at once; open one here when you want a closer look.
tools-section-space = Free up space
tools-section-health = PC health
tools-when-clean = Temporary files, caches and logs that Windows and your apps make again by themselves.
tools-when-devcache = You write code: package caches, build output and what AI coding tools leave behind.
tools-when-duplicates = The same photo, video or document saved more than once.
tools-when-programs = Remove a program you no longer use, together with what it leaves behind.
tools-when-oldlarge = Big files, and files you have not opened in a long time.
tools-when-tidy = Empty folders, empty files and shortcuts that lead nowhere.
tools-when-startup = The PC starts slowly: choose what opens when you sign in.
tools-when-health = Check that your drives are healthy and not too hot.
tools-when-hardware = Temperatures, fans, and what keeps the processor and memory busy.
tools-when-monitor = The disk is busy or keeps filling up: see which program writes to it.

## Quick clean: start

quick-title = Quick clean
quick-intro = One scan with every tool, one list of what can go, one confirmation.
quick-scan = Scan my PC
quick-scan-hint = If you do this: the app looks for junk, caches, empty folders and duplicates. It only reads; nothing is deleted.
quick-safety = Nothing is deleted until you confirm. What is cleaned can be restored from Cleanup history for 7 days.
quick-checks = What gets checked
quick-last = Last quick clean: freed { $size } on { $date }

## Quick clean: the scan

quick-scanning = Checking your PC
quick-scanning-sub = Every step only reads. You can keep using the app meanwhile.
quick-stop = Stop
quick-stop-hint = If you do this: the step that runs now stops and the rest is not started. Nothing is deleted.
quick-step-junk = Junk files
quick-step-dev = Developer caches
quick-step-bin = Recycle Bin
quick-step-scan = Scan of the system drive
quick-step-tidy = Empty folders and broken shortcuts
quick-step-advice = Safe suggestions from the scan
quick-step-large = Large and old files
quick-step-dupes = Duplicates in your folders
quick-status-waiting = Waiting
quick-status-running = Checking…
quick-status-skipped = Skipped
quick-status-failed = Could not finish
quick-status-cancelled = Stopped
quick-found-nothing = Nothing found
quick-found-items = { $count } items
quick-found-size-items = { $size } in { $count } items
quick-found-scan = { $count } files, { $size }
quick-found-advice = { $size } more than the junk above
quick-note-no-drive = No system drive was found to scan.
quick-note-needs-scan = Needs the drive scan, which did not finish.
quick-note-reused = Used the scan from a few minutes ago.
quick-busy = Another clean is running. Try again when it ends.
quick-step-failed = This step could not finish.
quick-advice-timeout = The suggestions took too long. Open the disk map to see them.

## Quick clean: the result

quick-ready-title = { $size } can be freed safely
quick-ready-title-upto = Up to { $size } can be freed safely
quick-ready-nothing = Nothing needs cleaning right now
quick-ready-sub = Ticked items go to the quarantine. You can restore them from Cleanup history for 7 days.
quick-upto-note = "Up to": some of these places can overlap, and files an open app is using stay where they are.
quick-stopped-note = You stopped the scan, so this list only shows the steps that finished.
quick-clean = Clean { $size }
quick-clean-upto = Clean up to { $size }
quick-clean-hint = If you do this: a confirmation lists what goes where. Nothing is deleted before you confirm.
quick-rescan = Scan again
quick-rescan-hint = If you do this: every step runs again. It only reads.
quick-how = How it was checked
quick-group-safe = Safe to remove
quick-group-safe-sub = Apps and Windows make these again when they need them. They are ticked for you.
quick-group-look = Worth a look
quick-group-look-sub = Only you can tell whether these can go. Nothing here is ticked.
quick-group-info = Good to know
quick-group-info-sub = Space this app leaves alone, and why.
quick-row-junk = Junk files
quick-row-junk-sub = Temporary files, caches and logs of { $count } apps.
quick-row-dev = Developer caches
quick-row-dev-sub = Package and build caches your tools download again when they need them.
quick-row-advice = More from the disk scan
quick-row-advice-sub = Build output and caches the scan recognized, apart from the junk above.
quick-row-empty = Empty folders
quick-row-empty-sub = { $count } folders with nothing in them.
quick-row-links = Broken shortcuts
quick-row-links-sub = { $count } shortcuts whose file or folder is gone.
quick-row-junk-caution = Junk that needs a look
quick-row-junk-caution-sub = Caches some apps take a while to rebuild, or logs you may still want. Tick it if you are sure.
quick-row-dupes = Duplicate files
quick-row-dupes-sub = { $count } groups of identical files in your folders. You choose which copy stays.
quick-row-large = Large and old files
quick-row-large-sub = { $count } files that are big or have not been opened for a long time.
quick-row-more = More suggestions
quick-row-more-sub = { $count } groups the disk map can explain, such as AI agent folders.
quick-row-bin = Recycle Bin
quick-row-bin-sub = { $count } items you deleted before. Emptying it is permanent.
quick-row-guide = Needs the proper Windows tool
quick-row-guide-sub = System space such as restore points and the page file. Only Windows' own settings shrink it safely.
quick-row-admin = Needs administrator rights
quick-row-admin-sub = Part of the junk is in system folders. After cleaning, Junk files can finish it with one Windows prompt.
quick-review = Review
quick-details = Details
quick-hide = Hide
quick-open-tool = Open the Windows tool
quick-close-apps = Close { $apps } first, or their files stay.
quick-more-lines = and { $count } more

## Quick clean: confirmation

quick-confirm-title = Clean { $size }?
quick-confirm-title-upto = Clean up to { $size }?
quick-confirm-intro = These parts run one after the other:
quick-where-quarantine = to the quarantine
quick-confirm-tidy-blocked = { $count } items stay: the safety check keeps them.
quick-confirm-note = Everything goes to the quarantine first. You can restore any of it from Cleanup history for 7 days; after that the quarantine deletes it by itself.
quick-confirm-ack = I looked at the "worth a look" junk I ticked and want it cleaned.
quick-confirm-go = Clean now
quick-confirm-go-hint = If you do this: the ticked items move to the quarantine, one tool after the other. Every item is checked again right before it moves.
quick-tool-advice = Suggestions from the disk scan
quick-tool-tidy = Empty folders and broken shortcuts
quick-tool-junk = Junk files
quick-tool-dev = Developer caches

## Quick clean: cleaning and the result

quick-cleaning = Cleaning
quick-cleaning-sub = Moving the ticked items to the quarantine.
quick-stop-clean-hint = If you do this: the running part stops after the current item; what already moved stays restorable.
quick-progress = { $done } of { $total } items, { $size } so far
quick-part-freed = Freed { $size }
quick-part-failed = Did not finish: { $error }
quick-part-not-run = This part did not run.
quick-done-title = Freed { $size }
quick-done-nothing = Nothing was freed
quick-done-sub = { $count } items moved to the quarantine. You can restore them from Cleanup history for 7 days.
quick-done-toast = Quick clean freed { $size } ({ $count } items).
quick-stayed-title = What stayed and why
quick-stayed-line = { $count } items stayed
quick-stayed-none = Everything you ticked was cleaned.
quick-finish-in-junk = Finish in Junk files
quick-history = See it in Cleanup history

## Home

home-quick-btn = Quick clean
home-quick-hint = If you do this: Quick clean opens and checks your PC. It only reads; nothing is deleted before you confirm.
home-quick-open-hint = If you do this: Quick clean opens with the result of your last check. Nothing is deleted before you confirm.
home-quick-ready = { $size } ready to clean
home-quick-never = Check your PC for junk, caches and duplicates in one go.

## First-run welcome

welcome-label = Welcome
welcome-title-1 = Welcome to NANOFreeDisk
welcome-body-1 = It shows what fills your drives and frees space safely: junk files, caches, duplicates and programs you no longer use.
welcome-title-2 = You stay in control
welcome-body-2 = Nothing is deleted without your confirmation, and cleaned items can be restored from Cleanup history for 7 days.
welcome-title-3 = Simple or Expert?
welcome-body-3 = You can switch at any time at the bottom of the sidebar.
welcome-simple-sub = A short menu and Quick clean. The safe choice for most people.
welcome-expert-sub = Every tool in the menu, advanced filters, exports and command lines.
welcome-step = Step { $n } of 3
welcome-next = Next
welcome-back = Back
welcome-skip = Skip
welcome-done = Start
settings-welcome-again = Show the welcome again
