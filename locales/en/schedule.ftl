# Settings, Automatic cleaning: low space alert, scheduled clean, start with
# Windows (docs/SCHEDULE.md).

schedule-title = Automatic cleaning
schedule-sub = Small jobs NANOFreeDisk can do on its own while it runs. Everything here stays off until you turn it on.
schedule-loading = Loading…

schedule-low-heading = Low disk space
schedule-low-toggle = Warn me when a drive is almost full
schedule-low-percent = Below this share of the drive (%)
schedule-low-gb = Or below this much free space (GB)
schedule-low-hint = The smaller of the two counts. You get one notification per drive, at most once a day, and the next one only after space was freed. While this is on, it replaces the low space warning of the drive health alerts.

schedule-clean = Clean automatically
schedule-clean-label = How often
schedule-freq-off = Off
schedule-freq-weekly = Weekly
schedule-freq-monthly = Monthly
schedule-idle = Only when the PC has been idle for a few minutes
schedule-what = Only Safe items that programs rebuild by themselves, such as temporary files and caches. Never anything that needs administrator rights, never the Recycle Bin, duplicates, Tidy up or what you picked in Analyze, and nothing of a program that is open. Items go to the quarantine: you can restore them from Cleanup history for 7 days.
schedule-categories = Categories that take part
schedule-needs-running = Automatic cleaning only runs while NANOFreeDisk is open or waiting in the tray. Turn on "Start NANOFreeDisk with Windows" below, or keep the app open. A clean that was missed runs a few minutes after the next start, once.
schedule-unsupported = Automatic cleaning is available on Windows. The low disk space warning works here too.

schedule-autostart-heading = Start with the computer
schedule-autostart = Start NANOFreeDisk with Windows (in the tray)
schedule-autostart-mac = Start NANOFreeDisk when you log in (in the menu bar)
schedule-autostart-hint = It starts hidden, with only its icon. Closing the window then keeps it running there; choose Quit in the icon's menu to end it. In Startup apps the entry is called NANOFreeDisk.
schedule-autostart-blocked = This entry is turned off in Task Manager's Startup apps. Turn it off and on again here to let it start.
schedule-autostart-on = NANOFreeDisk now starts with Windows, in the tray.
schedule-autostart-off = NANOFreeDisk no longer starts with Windows.

schedule-last = Last automatic clean: { $date }, freed { $size }
schedule-last-none = No automatic clean yet.
schedule-next = Next one not before { $date }
schedule-history = Cleanup history
schedule-run-now = Run now
schedule-busy = Working…
schedule-preview-title = An automatic clean would take this now
schedule-preview-total = { $count } items, { $size } in total
schedule-preview-where = They go to the quarantine. You can restore them from Cleanup history for 7 days.
schedule-preview-empty = Nothing to clean right now.
schedule-preview-go = Clean these now
schedule-preview-cancel = Cancel
schedule-preview-expired = That preview is too old. Choose Run now again to see what would be cleaned.
schedule-done = Cleaned { $size } automatically. See Cleanup history.

# rule categories in the picker
schedule-cat-ai-agent = AI agents
schedule-cat-ai-app = AI apps
schedule-cat-ai-ide = AI code editors
schedule-cat-browser = Browsers
schedule-cat-editor = Code editors
schedule-cat-game-engine = Game engines
schedule-cat-gpu = Graphics shader caches
schedule-cat-ide = IDEs
schedule-cat-llm-runtime = Local AI model runners
schedule-cat-ml-cache = Machine learning caches
schedule-cat-package-manager = Package managers
schedule-cat-sdk = SDKs
schedule-cat-system = Windows
schedule-cat-tool = Tools
schedule-cat-toolchain = Toolchains

# notifications and the tray menu (filled in by the app)
schedule-note-auto-title = Automatic cleaning
schedule-note-auto-body = Cleaned { $size } automatically. See Cleanup history.
schedule-note-low-title = { $drive } is almost full
schedule-note-low-body = { $free } free. Open NANOFreeDisk from the tray icon and choose Free up space.
schedule-tray-free-up = Free up space

activity-kind-clean-scheduled = Automatic clean
