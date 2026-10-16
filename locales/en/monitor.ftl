# Monitor — Disk Activity (docs/PHASE6_MONITOR.md). Read-only: nothing here ends
# or changes a process.

mon-title = Disk activity
mon-subtitle = What is reading and writing your disks right now, file by file when detail is on. Read-only — nothing is stopped or changed.
mon-tab-activity = Live
mon-tab-history = 24 hours
mon-tab-who = Who touched this?

mon-source-counters = I/O counters
mon-source-etw = File detail
mon-source-helper = File detail (admin helper)
mon-source-counters-hint = Each process's I/O counters: every read and write it makes, including network and pipes, without file names. No admin rights needed.
mon-source-etw-hint = A kernel trace session in this app (it runs as administrator): reads and writes per process and per file on local drives.
mon-source-helper-hint = A kernel trace session in the elevated helper: reads and writes per process and per file on local drives.

mon-detail-on = Show files (admin)
mon-detail-off = Stop file detail
mon-detail-declined = File detail needs administrator rights; Windows was told no.
mon-detail-failed = Could not start file detail: { $message }
mon-detail-error-etw-silent = The trace session reported no file activity while the disks were busy (security software may block it), so the monitor went back to I/O counters.
mon-detail-error-helper-stopped = The admin helper has stopped. Turn file detail on again to restart it.
mon-detail-error-session-stopped = The trace session was stopped from outside the app. Turn file detail on again to restart it.
mon-detail-error-other = File detail stopped. Turn it on again to retry.
mon-pause = Pause
mon-resume = Resume
mon-paused-note = The monitor is paused: nothing is sampled and no trace session runs.
mon-error = Could not read disk activity: { $message }
mon-loading = Measuring…

mon-read = Read
mon-write = Write
mon-r = R
mon-w = W
mon-ops = ops/s
mon-window = Time range
mon-window-60 = 60 s
mon-window-600 = 10 min
mon-ago-60 = 60 seconds ago
mon-ago-600 = 10 minutes ago
mon-chart-label = Read and write rate of all processes
mon-disk = Disk { $index }
mon-queue = Queue
mon-latency = Latency
mon-busy = { $pct } % busy
mon-no-disks = No physical disk counters.

mon-col-name = Process
mon-col-pid = PID
mon-col-read = Read/s
mon-col-write = Write/s
mon-col-iops = IOPS
mon-col-files = Files
mon-idle = No process is reading or writing right now.
mon-reveal = Show in Explorer
mon-reveal-exe = Show the program in Explorer
mon-files-need-detail = Turn on file detail to see which files this process uses.
mon-no-files = No file activity in the last second.
mon-process-gone = { $name } has already exited.

mon-top-written = Files being written
mon-top-need-detail = The I/O counters know processes, not files. Turn on file detail (admin) to see the files being written and jump to their folder.
mon-no-writes = Nothing is being written right now.
mon-lost = Trace session: { $lost } events dropped, { $undecoded } not decoded.

mon-history-title = Last 24 hours, by hour
mon-history-empty = Nothing recorded yet. Activity is saved per minute while this view is open or log-spam alerts are on, and only when scan history is on in Settings.
mon-history-hint = Kept for 24 hours on this PC; cleared with the scan history in Settings.
mon-top-writers = Wrote the most

mon-who-title = Who touched this?
mon-who-hint = Pick a folder to see which processes create, write, rename and delete files in it. Needs file detail.
mon-who-placeholder = Folder to watch, e.g. C:\Users\you\Documents
mon-who-pick = Choose a folder to watch
mon-who-start = Watch
mon-who-stop = Stop watching
mon-who-need-detail = File detail is off: turn it on to record the events.
mon-who-idle = No folder watched.
mon-who-none = Nothing has happened in this folder yet.
mon-op-create = Created
mon-op-write = Written
mon-op-delete = Deleted
mon-op-rename = Renamed

mon-alert-spam-title = { $name } keeps writing to disk
mon-alert-spam-body = { $name } has been writing { $rate } MB/s for { $minutes } minutes. Open Disk activity to see the files.
mon-alert-log-title = Large log file: { $name }
mon-alert-log-body = { $name } is writing to { $path } ({ $size } GB).
mon-alert-spam-toast = { $name } has been writing { $rate } for { $minutes } min
mon-alert-log-toast = { $name } is writing to a { $size } log file: { $path }

hint-mon-detail-on = Starts a kernel trace session in the admin helper (Windows asks for permission once; Advanced sensors share it). Nothing on the PC changes; the session stops when you pause, leave this view or close the app.
hint-mon-detail-on-admin = Starts a kernel trace session in this app (it already runs as administrator). Nothing on the PC changes; the session stops when you pause, leave this view or close the app.
hint-mon-detail-off = Stops the trace session; the view goes back to the I/O counters.
hint-mon-pause = Stops all sampling and the trace session until you resume. Alerts stop too.
hint-mon-resume = Starts sampling again (and file detail, if it was on).
hint-mon-end-task = Opens the End task dialog of Hardware → Tasks: it checks the process first, asks you to confirm and never ends system processes.
hint-mon-analyze = Scans this file's folder in Analyze (read-only).
hint-mon-watch = Records who creates, writes, renames and deletes files in this folder while file detail is on. Nothing is changed.
hint-mon-unwatch = Stops recording events for this folder.

settings-monitor = Disk activity
settings-mon-alerts = Warn me when a process keeps writing to disk
settings-mon-mbps = Faster than (MB/s)
settings-mon-minutes = For (minutes)
settings-mon-hint = Catches runaway loggers, indexers and sync clients. While on, the monitor samples once a second in the background (well under 1 % CPU); log files over 1 GB that are being written are reported too.
hint-mon-alerts = While on, the app samples disk activity every second in the background and shows a notification when a process writes faster than the limit for the whole time. It never stops the process.
hint-mon-threshold = Changes the log-spam alert limit.
