## Health view (Phase 7, docs/PHASE7_HEALTH.md)

health-title-view = Drive health
health-subtitle = A Good / Caution / Bad verdict for every drive from its own S.M.A.R.T. or NVMe health data, the way CrystalDiskInfo judges it, plus read-only checks of each volume. Nothing here changes a drive.
health-refresh = Read the drives again
health-elevate = Read everything (administrator)
health-elevate-hint = If you click this: Windows asks for administrator rights once; a helper reads S.M.A.R.T. thresholds, drives behind USB adapters and whether a volume needs a disk check, then closes. Read-only — nothing is written to any drive.
health-elevated-done = Drives read with administrator rights.
health-elevated-declined = Administrator rights were not granted; showing what a standard user can read.
health-export-html = Save report
health-export-html-hint = If you click this: saves a self-contained HTML report of the drives and volumes (no serial numbers unless you include them) to a file you choose.
health-export-json = Export JSON
health-export-json-hint = If you click this: saves the report as JSON (nanofreedisk.health.v1) to a file you choose.
health-export-title = Save drive health report
health-exported = Saved to { $path }
health-serials = Include serial numbers
health-serials-hint = If you tick this: exported reports include each drive's serial number (they identify your hardware — leave it off when sharing).
health-error = Could not read the drives: { $message }
health-loading = Reading the drives…
health-standard-user = Read as a standard user: S.M.A.R.T. thresholds, drives behind USB adapters and the disk-check flag need administrator rights.
health-no-drives = No physical drive could be listed.
health-drives = Drives
health-volumes = Volumes
health-no-problems = No problems found.
health-no-thresholds = Failure thresholds were not read (administrator rights needed), so only the raw counters were judged.

health-status-unknown = Unknown
health-status-good = Good
health-status-caution = Caution
health-status-bad = Bad
health-ssd = SSD
health-hdd = Hard disk

health-source-nvme = NVMe health log
health-source-ata-driver = S.M.A.R.T. (through the storage driver)
health-source-ata-smart = S.M.A.R.T. with thresholds
health-source-sat = S.M.A.R.T. through the USB adapter
health-source-none = No health data

health-note-usb-bridge = This USB adapter does not pass S.M.A.R.T. through (or administrator rights are needed to try). The drive is not necessarily unhealthy.
health-note-needs-admin = Thresholds and full S.M.A.R.T. need administrator rights — use "Read everything".
health-note-raid = RAID / Storage Spaces: the member drives cannot be read through the array.
health-note-virtual = Virtual drive: it has no health data of its own.
health-note-no-smart = This drive reports no S.M.A.R.T. or NVMe health data.

health-reason-nvme-spare-low = Spare area is down to { $value } %, below the drive's own threshold — back up and plan a replacement.
health-reason-nvme-reliability = The drive reports that its reliability has degraded (media or internal errors) — back up now.
health-reason-nvme-read-only = The drive has switched itself to read-only mode — copy your data off it.
health-reason-nvme-backup = The drive's power-loss backup has failed.
health-reason-nvme-temperature = The drive reports a temperature warning.
health-reason-nvme-media-errors = { $value } media or data-integrity errors were recorded.
health-reason-worn = Only { $value } % of the drive's rated life is left.
health-reason-ata-predict-failure = The drive itself predicts a failure — back up now.
health-reason-ata-threshold = Attribute { $attr } { $name } fell below its failure threshold (now { $value }).
health-reason-ata-reallocated = { $value } bad sectors were already replaced by spares.
health-reason-ata-pending = { $value } sectors could not be read and wait to be replaced.
health-reason-ata-uncorrectable = { $value } sectors could not be recovered.

health-temperature = Temperature
health-temp-limits = (warning at { $warning } °C, critical at { $critical } °C)
health-life = Life left
health-power-on = Power-on time
health-hours = { $hours } h ({ $days } days)
health-minutes = { $n } min
health-power-cycles = Power cycles
health-written = Data written
health-read = Data read
health-firmware = Firmware
health-serial = Serial number
health-trim-support = TRIM supported
health-yes = Yes
health-no = No
health-none = None
health-not-checked = Not checked (administrator)

health-nvme-warning = Critical warning
health-nvme-spare = Available spare
health-nvme-spare-value = { $spare } % (threshold { $threshold } %)
health-nvme-used = Percentage used
health-nvme-media-errors = Media errors
health-nvme-unsafe = Unsafe shutdowns
health-nvme-read = Data units read
health-nvme-written = Data units written
health-nvme-host-reads = Host read commands
health-nvme-host-writes = Host write commands
health-nvme-busy = Controller busy time
health-nvme-error-log = Error log entries
health-nvme-warn-time = Time above warning temperature
health-nvme-crit-time = Time above critical temperature
health-nvme-sensors = Temperature sensors

health-attr-name = Attribute
health-attr-current = Current
health-attr-worst = Worst
health-attr-threshold = Threshold
health-attr-raw = Raw
health-attr-vendor = Vendor specific

health-vol-free = Free space
health-vol-forecast = Full in
health-vol-full-in = about { $days } days at the current pace
health-vol-not-filling = not filling up
health-vol-no-trend = not enough scan history yet
health-vol-fs = File system
health-vol-cluster = Cluster size
health-vol-aligned = Partition aligned (4 KB)
health-vol-trim = Windows sends TRIM
health-vol-bitlocker = BitLocker
health-vol-dirty = Needs a disk check
health-bitlocker-on = On
health-bitlocker-off = Off
health-bitlocker-encrypting = Encrypting
health-bitlocker-decrypting = Decrypting
health-bitlocker-suspended = Suspended
health-bitlocker-locked = Locked
health-bitlocker-waiting = Waiting for activation

health-col-bus = Connection
health-col-size = Size
health-report-title = Drive health report
health-report-standard-user = read as a standard user
health-report-footer = Made with NANOFreeDisk — read-only diagnostics; nothing was changed on these drives.

## Health Score findings (Home)

health-drive-bad = { $folder } is failing — back up your data now.
health-drive-caution = { $folder } needs attention (S.M.A.R.T. / NVMe health) — keep a backup.
health-drive-hot = { $folder } is at { $value } °C, at or above its own critical temperature.
health-drive-warm = { $folder } is at { $value } °C, above its own warning temperature.
health-drive-unknown = The health of { $folder } could not be read.
health-volume-dirty = { $drive } is marked for a disk check (chkdsk) — Windows will check it at the next restart.
health-fix-health = See drive health
