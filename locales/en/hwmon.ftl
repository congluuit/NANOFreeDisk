# Hardware view (docs/HARDWARE_MONITOR.md): Overview, Sensors, Tasks.

nav-hardware = Hardware
hw-title = Hardware
hw-subtitle = Your machine's parts, live sensors and running tasks — read-only, no driver needed.
hw-tab-overview = Overview
hw-tab-sensors = Sensors
hw-tab-tasks = Tasks
hw-refresh = Refresh
hw-loading = Reading the hardware…
hw-error = Could not read the hardware: { $message }
hw-copy-text = Copy as text
hw-copied = Hardware summary copied to the clipboard.
hw-export-json = Export JSON
hw-export-title = Save the hardware inventory
hw-exported = Inventory saved to { $path }.
hw-text-header = NANOFreeDisk hardware summary — { $date }
hw-privacy = Serial numbers, MAC and IP addresses are never collected, so the summary is safe to share.
hw-none = Nothing detected.

hw-sec-system = System
hw-sec-cpu = Processor
hw-sec-board = Mainboard & BIOS
hw-sec-memory = Memory
hw-sec-gpu = Graphics
hw-sec-storage = Storage
hw-sec-display = Displays
hw-sec-network = Network
hw-sec-battery = Battery

hw-model = Model
hw-chassis = Type
hw-chassis-laptop = Laptop
hw-chassis-desktop = Desktop
hw-chassis-server = Server
hw-chassis-tablet = Tablet
hw-chassis-handheld = Handheld
hw-os = Windows
hw-os-build = Build
hw-computer = Computer name
hw-uptime = Uptime
hw-uptime-value = { $days } d { $hours } h { $minutes } min

hw-cores = Cores
hw-cores-hybrid = { $cores } ({ $p } performance + { $e } efficiency)
hw-threads = Threads
hw-clock-base = Base clock
hw-clock-max = Max boost
hw-ghz = { $value } GHz
hw-mhz = { $value } MHz
hw-cache = Cache
hw-socket = Socket
hw-cpuid = CPUID
hw-family = Family
hw-model-id = Model
hw-stepping = Stepping

hw-board = Board
hw-board-version = Version
hw-bios = BIOS
hw-bios-date = BIOS date
hw-smbios = SMBIOS

hw-mem-installed = Installed
hw-mem-usable = Usable by Windows
hw-mem-in-use = In use
hw-mem-in-use-value = { $used } ({ $percent } %)
hw-mem-slots = Slots
hw-mem-slots-value = { $used } of { $total } used
hw-mem-max = Maximum supported
hw-mem-speed = Speed
hw-mem-rated = Rated speed
hw-mts = { $value } MT/s
hw-maker = Manufacturer
hw-part = Part number
hw-rank = Ranks
hw-bank = Bank

hw-vram = Video memory
hw-driver = Driver
hw-driver-date = Driver date
hw-location = Location

hw-capacity = Capacity
hw-disk-type = Type
hw-disk-n = Disk { $n }
hw-removable = removable
hw-temperature = Temperature
hw-firmware = Firmware

hw-display-builtin = Built-in display
hw-display-generic = Display
hw-primary = Primary
hw-resolution = Resolution
hw-refresh-rate = Refresh rate
hw-hz = { $value } Hz
hw-connector = Connection

hw-net-wifi = Wi-Fi
hw-net-ethernet = Ethernet
hw-net-status = Status
hw-net-up = Connected
hw-net-up-speed = Connected — { $mbps } Mbps
hw-net-down = Not connected
hw-net-alias = Windows name

hw-bat-charge = Charge
hw-bat-health = Health
hw-bat-health-value = { $percent } % of design capacity (worn { $wear } %)
hw-bat-capacity = Capacity
hw-bat-capacity-value = { $full } of { $design } mWh
hw-bat-cycles = Charge cycles
hw-bat-chemistry = Chemistry

## Sensors tab

hw-sensors-pawn-hint = CPU temperatures, power and voltages need Advanced sensors (PawnIO).
hw-sensors-stats = { $count } sensors · sampled in { $ms } ms
hw-reset-minmax = Reset min/max
hw-col-sensor = Sensor
hw-col-value = Value
hw-col-min = Min
hw-col-max = Max
hw-estimated = estimated
hw-estimated-hint = Approximation: Windows' "% Processor Performance" × base clock. Exact clocks need Advanced sensors.
hw-core-p = P-core
hw-core-e = E-core
hw-kind-temperature = Temperatures
hw-kind-voltage = Voltages
hw-kind-power = Powers
hw-kind-clock = Clocks
hw-kind-load = Load
hw-kind-fan = Fans
hw-kind-control = Fan duty
hw-kind-level = Levels
hw-kind-energy = Capacity
hw-kind-data = Data
hw-kind-throughput = Throughput
hw-note-needs-pawnio = Needs Advanced sensors (PawnIO) — not available without it.
hw-note-gpu-asleep = The GPU is asleep to save power; its values appear when it wakes up.
hw-note-not-reported = The driver does not report this.

hw-s-cpu-total = CPU Total
hw-s-cpu-thread-max = Busiest thread
hw-s-cpu-thread = Thread #{ $n }
hw-s-cpu-clock-avg = Average clock while busy
hw-s-mem-load = Memory
hw-s-mem-used = Memory used
hw-s-mem-available = Memory available
hw-s-mem-commit-load = Virtual memory
hw-s-mem-commit-used = Virtual memory used
hw-s-mem-commit-limit = Virtual memory limit
hw-s-acpi-zone = Thermal zone { $detail }
hw-s-gpu-temp = GPU Core
hw-s-gpu-clock-core = GPU Core
hw-s-gpu-clock-mem = GPU Memory
hw-s-gpu-power = GPU Package
hw-s-gpu-power-limit = Power limit
hw-s-gpu-fan-duty = GPU Fan
hw-s-gpu-fan = GPU Fan
hw-s-gpu-load = GPU Core
hw-s-gpu-load-3d = 3D
hw-s-gpu-load-copy = Copy
hw-s-gpu-load-decode = Video Decode
hw-s-gpu-load-encode = Video Encode
hw-s-gpu-load-compute = Compute
hw-s-gpu-mem-used = Dedicated memory used
hw-s-gpu-mem-shared = Shared memory used
hw-s-gpu-mem-load = Dedicated memory
hw-s-gpu-mem-total = Dedicated memory total
hw-s-disk-temp = Temperature
hw-s-disk-active = Active time
hw-s-disk-read = Read rate
hw-s-disk-write = Write rate
hw-s-bat-charge = Charge level
hw-s-bat-wear = Wear level
hw-s-bat-rate = Charge / discharge rate
hw-s-bat-voltage = Voltage
hw-s-bat-remaining = Remaining capacity
hw-s-bat-full = Full charge capacity
hw-s-bat-design = Design capacity
hw-s-net-down = Download speed
hw-s-net-up = Upload speed

## Home temperature card

home-temps-title = Temperatures
home-temps-open = Open Hardware → Sensors
home-temp-cpu = CPU
home-temp-gpu = GPU
home-temp-disk = Hottest disk
home-temp-load = load { $percent } %
home-temp-why-needs-pawnio = needs Advanced sensors — click to set up
home-temp-why-gpu-asleep = GPU asleep (saving power)
home-temp-why-not-reported = not reported
home-temp-why-none = not available

## Tasks tab

task-sum-cpu = CPU
task-sum-memory = Memory
task-sum-disk = Disk
task-sum-gpu = GPU
task-sum-processes = Processes
task-search = Search name, PID, publisher…
task-tree = Tree
task-pause = Pause
task-pause-hint = Stop refreshing the list (the selection stays put)
task-hint = Right-click a process for actions. Nothing is ever ended automatically.
task-expand = Expand or collapse
task-group-app = Apps
task-group-background = Background processes
task-group-windows = Windows processes
task-col-name = Name
task-col-pid = PID
task-col-cpu = CPU
task-col-memory = Memory
task-col-disk = Disk
task-col-gpu = GPU
task-col-publisher = Publisher
task-col-threads = Threads
task-col-handles = Handles
task-col-path = Path
task-menu-close = Close app
task-menu-kill = End process…
task-menu-kill-tree = End process tree…
task-menu-blocked = Windows needs this process — it cannot be ended here
task-menu-reveal = Open file location
task-menu-copy-path = Copy path
task-menu-copy-cmd = Copy command line
task-menu-taskmgr = Open Task Manager
task-copied = Copied to the clipboard.
task-no-cmd = The command line of this process cannot be read.

task-title-close = Close this app?
task-title-kill = End this process?
task-title-kill-tree = End this process and everything it started?
task-name = Process
task-publisher = Publisher
task-publisher-unknown = unknown
task-signed = signed
task-path = File
task-children = Child processes
task-what-close = The app is asked to close, as if you clicked its ✕. It may ask to save your work first.
task-what-kill = The process stops immediately. Anything it has not saved is lost.
task-what-kill-tree = The process and all the processes it started stop immediately. Anything they have not saved is lost.
task-warn-service-host = This process hosts Windows services. Ending it stops all of them; Windows may restart some, others stay off until you restart.
task-warn-explorer = This is Windows Explorer: the taskbar, Start menu and desktop disappear until it starts again. Restarting it from Task Manager is usually the better choice.
task-warn-windows-component = This program is part of Windows. Ending it can make Windows unstable until you restart.
task-ack = I understand — end it anyway
task-no-undo = This cannot be undone. The action is recorded in the activity log.
task-cancel = Cancel
task-working = Working…
task-confirm-close = Close app
task-confirm-kill = End process
task-confirm-kill-tree = End process tree
task-close-sent = Asked "{ $name }" to close.
task-close-timeout = "{ $name }" did not close within 5 seconds.
task-ended = Ended { $count ->
    [one] { $name }
   *[other] { $count } processes
}.
task-not-ended = Not ended: { $name } — { $reason }
task-block-kernel = This is the Windows kernel.
task-block-system-core = Windows cannot run without this process.
task-block-critical = Windows marks this process as critical: ending it would crash the system.
task-block-protected = This is a protected process (for example antivirus); Windows does not allow ending it.
task-block-own = This is NANOFreeDisk itself.
task-outcome-changed = The process has already ended or its PID now belongs to another process. Nothing was done.
task-outcome-access-denied = The process belongs to another user or runs as administrator. Ending it needs administrator rights.
task-outcome-no-window = The process has no window to close. Use End process instead.
task-outcome-needs-confirm = It needs the second confirmation.

## Advanced sensors (PawnIO, ADR 0002)

hw-adv-on = Advanced sensors on (PawnIO, read-only)
hw-adv-not-installed = CPU temperatures, power and voltages need PawnIO, the free signed driver also used by LibreHardwareMonitor and Fan Control.
hw-adv-not-running = PawnIO is installed but its driver is not running. If Memory Integrity blocks it, Windows Security shows why.
hw-adv-no-module = PawnIO is installed; the official module for this CPU (IntelMSR or AMDFamily17) still has to be downloaded.
hw-adv-setup = Set up Advanced sensors
hw-adv-setup-hint = Downloads PawnIO's official installer and the module for this CPU from its author's GitHub releases, checks their fingerprints, then installs PawnIO — Windows asks for administrator rights. Only reads sensors; nothing is ever written to the hardware.
hw-adv-setup-module-hint = Downloads the official module for this CPU from PawnIO's GitHub releases and checks its fingerprint.
hw-adv-setting-up = Setting up…
hw-adv-about = About PawnIO
hw-adv-about-hint = pawnio.eu — the free, signed driver also used by LibreHardwareMonitor and Fan Control
hw-adv-restart = PawnIO is installed. Restart Windows, then turn on Advanced sensors here.
hw-adv-setup-declined = PawnIO was not installed: administrator rights were not granted.
hw-adv-download-failed = Could not download PawnIO: { $message }
hw-adv-setup-failed = Advanced sensors could not be set up: { $message }
hw-adv-auto = Turn on automatically when the app opens
hw-adv-auto-hint = Windows asks for administrator rights once each time NANOFreeDisk opens. Untick to turn Advanced sensors on only from here.
hw-adv-start = Turn on Advanced sensors
hw-adv-start-hint = Windows asks for administrator rights once; a small helper then reads CPU sensors through PawnIO. Nothing is ever written to the hardware.
hw-adv-starting = Waiting for Windows…
hw-adv-declined = Advanced sensors stay off: administrator rights were not granted.
hw-adv-failed = Advanced sensors could not start: { $message }
hw-adv-unavailable = Advanced sensors are not available on this PC ({ $reason }).
hw-note-helper-stopped = The Advanced sensors helper stopped; turn it on again from this tab.
home-temp-why-helper-stopped = Advanced sensors stopped

hw-s-cpu-package = CPU Package
hw-s-cpu-core-max = Core Max
hw-s-cpu-core-avg = Core Average
hw-s-cpu-core-temp = Core #{ $n }
hw-s-cpu-tjmax = TjMax
hw-s-cpu-power-package = CPU Package
hw-s-cpu-power-cores = CPU Cores
hw-s-cpu-power-gt = CPU Graphics
hw-s-cpu-power-dram = CPU Memory
hw-s-cpu-power-psys = Platform (PSys)
hw-s-cpu-pl1 = Power limit PL1 (long)
hw-s-cpu-pl2 = Power limit PL2 (short)
hw-s-cpu-vid-max = CPU Core (max VID)
hw-s-cpu-vid = Core #{ $n } VID
hw-s-cpu-clock-eff = Effective clock (average)
hw-s-cpu-clock-core = Core #{ $n }
hw-s-cpu-bus = Bus speed
hw-s-cpu-tdie = Core (Tdie)
hw-s-cpu-ccd = CCD #{ $n }
hw-s-cpu-ccd-max = CCDs Max
hw-s-cpu-power-core = Core #{ $n }

## Temperature alerts, 24-hour log, Health Score (slice 6)

hw-alerts = Temperature alerts
hw-alerts-hint = Notify me when the CPU, a GPU or a disk gets too hot (CPU 95 °C, GPU 90 °C, disk 70 °C). While on, sensors are read every 5 seconds even when this view is closed.
hw-alert-title = { $device } is very hot
hw-alert-body = { $value } °C (limit { $limit } °C). Check the fans and air vents, and close heavy apps if you do not need them.
hw-alert-toast = { $part } { $device } is at { $value } °C (limit { $limit } °C).
hw-part-cpu = CPU
hw-part-gpu = GPU
hw-part-disk = Disk
hw-24h-title = Last 24 hours
hw-24h-peak = max { $value } °C
hw-24h-empty = Nothing recorded yet. Temperatures are logged while this view is open or temperature alerts are on.
hw-24h-ago = 24 h ago
hw-24h-now = now

health-hot-cpu = The processor ({ $folder }) reached { $value } °C in the last 24 hours — check its cooling.
health-hot-gpu = The graphics card ({ $folder }) reached { $value } °C in the last 24 hours — check its cooling.
health-hot-disk = { $folder } reached { $value } °C in the last 24 hours; long periods that hot shorten a drive's life.
health-warm-cpu = The processor ({ $folder }) ran warm, up to { $value } °C, in the last 24 hours.
health-warm-gpu = The graphics card ({ $folder }) ran warm, up to { $value } °C, in the last 24 hours.
health-warm-disk = { $folder } ran warm, up to { $value } °C, in the last 24 hours.
health-fix-hardware = See temperatures
hw-s-disk-temp-warning = Warning temperature (drive)
hw-s-disk-temp-critical = Critical temperature (drive)
