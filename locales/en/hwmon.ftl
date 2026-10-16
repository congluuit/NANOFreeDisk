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
hw-s-cpu-clock-avg = Average clock
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
home-temp-acpi = ACPI thermal zone (approximate)
home-temp-load = load { $percent } %
home-temp-why-needs-pawnio = needs Advanced sensors
home-temp-why-gpu-asleep = GPU asleep (saving power)
home-temp-why-not-reported = not reported
home-temp-why-none = not available
