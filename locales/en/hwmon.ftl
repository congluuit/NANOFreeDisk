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
