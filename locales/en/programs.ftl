# Programs view: installed programs, their own uninstallers, leftovers after
# uninstalling, software junk (docs/UNINSTALLER.md).

nav-programs = Programs
pg-title = Programs
pg-intro = Uninstall programs with their own uninstaller, then remove what they leave behind. Software junk lists the caches, logs and crash dumps installed programs pile up.
pg-safety-note = Nothing is gone for good: files go to the quarantine, registry keys to a .reg backup — undo from the Activity log.
pg-tab-installed = Installed
pg-tab-junk = Software junk

# installed list
pg-search = Search programs
pg-filter = Show
pg-filter-all = All programs
pg-filter-win32 = Desktop programs
pg-filter-store = Store apps
pg-filter-user = Only for me
pg-filter-large = Large (1 GB+)
pg-filter-broken = Uninstaller missing
pg-sort = Sort
pg-sort-name = By name
pg-sort-size = By size
pg-sort-date = By install date
pg-store = Store apps
pg-refresh = Refresh
pg-refresh-hint = Read the list of installed programs again.
pg-summary = { $count } programs, { $size } in total · { $shown } shown
pg-loading = Reading the installed programs…
pg-none = No program matches.
pg-pick = Select { $name }
pg-unknown-publisher = Unknown publisher
pg-badge-store = Store
pg-badge-machine = All users
pg-badge-machine-hint = Installed for every user: Windows asks for administrator rights to remove it.
pg-badge-blocked = Kept
pg-caution-runtime = Runtime
pg-caution-runtime-hint = Other programs may need this runtime: remove it only if you know none does.
pg-caution-driver = Driver
pg-caution-driver-hint = Hardware software: a device may stop working until you install it again.
pg-caution-windows = Windows tool
pg-caution-windows-hint = Part of Windows' own tooling: Windows may install it again.
pg-broken = Uninstaller missing
pg-broken-hint = Its uninstaller is no longer on the disk: only what it left behind can be removed.
pg-size-measured = Size of its install folder (the program registered none).
pg-size-registered = Size the program registered with Windows.
pg-reveal = Show in Explorer
pg-uninstall = Uninstall
pg-uninstall-hint = Runs the program's own uninstaller, then looks for what it leaves behind.
pg-leftovers-only = Remove leftovers
pg-leftovers-only-hint = The uninstaller is gone: look for the program's folders, shortcuts and registry keys.
pg-uninstall-n = Uninstall { $count ->
    [one] 1 program
   *[other] { $count } programs
}
pg-uninstall-n-hint = The ticked programs are uninstalled one after the other, each with its own uninstaller.
pg-blocked-self = NANOFreeDisk is removed from Windows Settings or the Start menu.
pg-blocked-webview2 = Needed by NANOFreeDisk and many other programs.
pg-blocked-no-remove = The program says it cannot be removed.
pg-blocked-no-uninstaller = The program registered no way to uninstall it.
pg-blocked-non-removable = Part of Windows: it cannot be removed.

# uninstall confirmation and progress
pg-confirm-title = Uninstall { $count ->
    [one] 1 program
   *[other] { $count } programs
}?
pg-confirm-body = About { $size } in total.
pg-confirm-broken = (uninstaller missing: leftovers only)
pg-confirm-step-run = The program's own uninstaller opens in its window: follow it.
pg-confirm-step-scan = When it has finished, NANOFreeDisk looks for what was left behind.
pg-confirm-step-review = You choose what to remove; nothing goes without your tick.
pg-confirm-machine = Programs installed for every user: Windows asks for administrator rights.
pg-confirm-ack = I understand the warnings above.
pg-quiet = Uninstall silently when the vendor supports it
pg-confirm-go = Uninstall { $count ->
    [one] 1 program
   *[other] { $count } programs
}
pg-running = Uninstalling { $name }…
pg-running-hint = Follow the uninstaller's window. When it is done, the leftovers are looked for.
pg-scanning = Looking for what { $name } left behind…
pg-scanning-hint = Program folders, data folders, shortcuts and registry keys.
pg-queue = { $count } more waiting
pg-finished = It is finished
pg-finished-hint = The uninstaller is done (or stuck): stop waiting and look for leftovers.
pg-declined = { $name } was not uninstalled: the administrator prompt was declined.
pg-failed = { $name } could not be uninstalled: { $message }
pg-still-installed = { $name } is still installed.
pg-uninstalled = { $name } was uninstalled.
pg-still-title = { $name } is still installed
pg-still-body = Its uninstaller was cancelled or did not finish, so nothing is looked for. Run it again when you are ready.
pg-forced = Look for leftovers anyway
pg-forced-hint = Expert: list what it would leave behind. Nothing is ticked, and its entry is never offered.

# leftovers
pg-left-title-after-uninstall = { $name } left these behind
pg-left-title-broken = Leftovers of { $name } (uninstaller missing)
pg-left-title-forced = What { $name } would leave behind (still installed)
pg-left-intro = Safe items are ticked. Each one shows why it belongs to the program.
pg-left-none = Nothing was left behind. 🎉
pg-left-dismiss = Close without removing anything
pg-left-group-files = Files and folders
pg-left-group-shortcuts = Shortcuts
pg-left-group-registry = Registry
pg-left-group-services = Services (information)
pg-left-pick = Select
pg-left-blocked = Kept
pg-left-also = the name also fits { $name }
pg-left-large = large: look inside first — it may hold your own files (saves, libraries, chats)
pg-left-machine-hint = For every user: removed with administrator rights.
pg-left-admin = { $count ->
    [one] 1 item is
   *[other] { $count } items are
} for every user: Windows asks for administrator rights once.
pg-left-remove = Remove { $count } ({ $size })
pg-left-remove-hint = Files go to the quarantine (7 days), registry keys to a .reg backup; undo from the Activity log.
pg-left-nothing-picked = Nothing is ticked.
pg-left-done = Leftovers removed: { $done } items, { $size } freed.
pg-admin-declined = { $count } all-users items were kept: the administrator prompt was declined.
pg-reason-install-dir = was its install folder
pg-reason-same-name = named like the program
pg-reason-vendor-folder = the publisher's folder, empty without the program
pg-reason-weak-name = a longer name starting like the program
pg-reason-shortcut = shortcut to a file that is gone
pg-reason-start-menu-folder = Start menu folder of the program
pg-reason-arp-entry = its entry in Installed apps
pg-reason-software-key = its settings in the registry
pg-reason-vendor-key = the publisher's key, empty without the program
pg-reason-app-path = how Windows found its program file
pg-reason-run-entry = starts its missing program at sign-in
pg-reason-service = a service still points to it; remove it as administrator
pg-reason-store-data = the Store app's data folder

# software junk
pg-junk-intro = Caches, logs, crash dumps and old updates of installed programs. App data, settings, sign-ins and site data are never listed; what the Junk files view cleans is not counted twice.
pg-junk-rescan = Rescan
pg-junk-loading = Looking through the programs' data folders…
pg-junk-none = No software junk found.
pg-junk-unknown = No program
pg-junk-unknown-hint = No installed program claims this folder; the junk was recognized by its structure.
pg-junk-running = Running
pg-junk-running-hint = Close { $apps } first: files in use are skipped.
pg-junk-what-chromium-cache = App cache
pg-junk-what-service-worker = Offline web cache
pg-junk-what-electron-logs = App logs
pg-junk-what-crashpad = Crash reports
pg-junk-what-crash-dumps = Crash dumps
pg-junk-what-crashes = Crash files
pg-junk-what-dump-file = Crash dump
pg-junk-what-logs = Logs
pg-junk-what-temp = Temporary files
pg-junk-what-old-version = Old version
pg-junk-what-update-package = Downloaded update
pg-junk-what-update-temp = Update leftovers
pg-junk-what-pending-update = Downloaded installer
pg-junk-impact-cache = Rebuilt automatically; the app may start a little slower once.
pg-junk-impact-crash = Only used to report crashes; the app does not need them.
pg-junk-impact-log = Diagnostic logs; some apps keep useful history in them.
pg-junk-impact-temp = Temporary files; an app that is running may still use some.
pg-junk-impact-update = The app keeps the newest version; older ones only allow rolling back.
pg-junk-clean = Clean { $count } ({ $size })
pg-junk-clean-hint = To the quarantine for 7 days; undo from the Activity log.
pg-junk-confirm-title = Clean the software junk?
pg-junk-confirm-body = The ticked items go to the quarantine. Close the programs that use them first; files in use are skipped.
pg-junk-confirm-go = Clean
pg-junk-done = Software junk cleaned: { $done } items, { $size } freed.

# Activity log and reasons
activity-kind-uninstall = Leftovers of an uninstalled program
activity-kind-software-junk = Software junk cleaned
activity-mode-registry = registry (.reg backup)
activity-restore-needs-admin = needs administrator rights
activity-restore-admin-confirm = { $count ->
    [one] 1 item was
   *[other] { $count } items were
} for every user. Put { $count ->
    [one] it
   *[other] them
} back with administrator rights? Windows will ask.
reason-other-program = Belongs to (or holds) another installed program.
reason-info = Shown for information only.
reason-registry-protected = A registry location Windows keeps.
reason-not-all-users = Not an all-users location.
reason-uac-declined = The administrator prompt was declined.
reason-elevation-failed = The administrator step did not run.
reason-backup = The registry backup could not be written, so nothing was deleted.
reason-delete = The registry key could not be deleted.
reason-access-denied = Administrator rights are needed.
reason-missing = Already gone.
reason-registry = A registry item.
reason-unexpected-key = The backup holds other registry keys than the ones removed.
reason-unexpected-value = The backup holds other registry values than the one removed.
