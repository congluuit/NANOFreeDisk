# Màn hình Phần cứng (docs/HARDWARE_MONITOR.md): Tổng quan, Cảm biến, Tasks.

nav-hardware = Phần cứng
hw-title = Phần cứng
hw-subtitle = Linh kiện của máy, cảm biến trực tiếp và các tiến trình đang chạy — chỉ đọc, không cần driver.
hw-tab-overview = Tổng quan
hw-tab-sensors = Cảm biến
hw-tab-tasks = Tasks
hw-refresh = Làm mới
hw-loading = Đang đọc thông tin phần cứng…
hw-error = Không đọc được phần cứng: { $message }
hw-copy-text = Sao chép dạng văn bản
hw-copied = Đã sao chép tóm tắt phần cứng vào clipboard.
hw-export-json = Xuất JSON
hw-export-title = Lưu danh mục phần cứng
hw-exported = Đã lưu danh mục vào { $path }.
hw-text-header = Tóm tắt phần cứng NANOFreeDisk — { $date }
hw-privacy = Không bao giờ thu thập số serial, địa chỉ MAC hay IP, nên bản tóm tắt an toàn để chia sẻ.
hw-none = Không phát hiện thiết bị nào.

hw-sec-system = Hệ thống
hw-sec-cpu = Bộ xử lý
hw-sec-board = Bo mạch chủ & BIOS
hw-sec-memory = Bộ nhớ RAM
hw-sec-gpu = Đồ họa
hw-sec-storage = Ổ lưu trữ
hw-sec-display = Màn hình
hw-sec-network = Mạng
hw-sec-battery = Pin

hw-model = Model
hw-chassis = Loại máy
hw-chassis-laptop = Laptop
hw-chassis-desktop = Máy bàn
hw-chassis-server = Máy chủ
hw-chassis-tablet = Máy tính bảng
hw-chassis-handheld = Thiết bị cầm tay
hw-os = Windows
hw-os-build = Bản dựng
hw-computer = Tên máy
hw-uptime = Thời gian chạy
hw-uptime-value = { $days } ngày { $hours } giờ { $minutes } phút

hw-cores = Số nhân
hw-cores-hybrid = { $cores } ({ $p } nhân hiệu năng + { $e } nhân tiết kiệm)
hw-threads = Số luồng
hw-clock-base = Xung cơ bản
hw-clock-max = Xung tối đa
hw-ghz = { $value } GHz
hw-mhz = { $value } MHz
hw-cache = Bộ nhớ đệm
hw-socket = Socket
hw-cpuid = CPUID
hw-family = Family
hw-model-id = Model
hw-stepping = Stepping

hw-board = Bo mạch
hw-board-version = Phiên bản
hw-bios = BIOS
hw-bios-date = Ngày BIOS
hw-smbios = SMBIOS

hw-mem-installed = Đã lắp
hw-mem-usable = Windows dùng được
hw-mem-in-use = Đang dùng
hw-mem-in-use-value = { $used } ({ $percent } %)
hw-mem-slots = Khe cắm
hw-mem-slots-value = Đã dùng { $used }/{ $total }
hw-mem-max = Hỗ trợ tối đa
hw-mem-speed = Tốc độ
hw-mem-rated = Tốc độ định mức
hw-mts = { $value } MT/s
hw-maker = Hãng sản xuất
hw-part = Mã linh kiện
hw-rank = Số rank
hw-bank = Bank

hw-vram = Bộ nhớ đồ họa
hw-driver = Driver
hw-driver-date = Ngày driver
hw-location = Vị trí

hw-capacity = Dung lượng
hw-disk-type = Loại
hw-disk-n = Ổ { $n }
hw-removable = tháo rời được
hw-temperature = Nhiệt độ
hw-firmware = Firmware

hw-display-builtin = Màn hình tích hợp
hw-display-generic = Màn hình
hw-primary = Chính
hw-resolution = Độ phân giải
hw-refresh-rate = Tần số quét
hw-hz = { $value } Hz
hw-connector = Kết nối

hw-net-wifi = Wi-Fi
hw-net-ethernet = Ethernet
hw-net-status = Trạng thái
hw-net-up = Đã kết nối
hw-net-up-speed = Đã kết nối — { $mbps } Mbps
hw-net-down = Chưa kết nối
hw-net-alias = Tên trong Windows

hw-bat-charge = Mức pin
hw-bat-health = Sức khỏe pin
hw-bat-health-value = Còn { $percent } % dung lượng thiết kế (chai { $wear } %)
hw-bat-capacity = Dung lượng
hw-bat-capacity-value = { $full }/{ $design } mWh
hw-bat-cycles = Số lần sạc
hw-bat-chemistry = Loại pin

## Tab Cảm biến

hw-sensors-pawn-hint = Nhiệt độ, công suất và điện áp CPU cần Cảm biến nâng cao (PawnIO).
hw-sensors-stats = { $count } cảm biến · lấy mẫu trong { $ms } ms
hw-reset-minmax = Đặt lại min/max
hw-col-sensor = Cảm biến
hw-col-value = Giá trị
hw-col-min = Min
hw-col-max = Max
hw-estimated = ước tính
hw-estimated-hint = Giá trị xấp xỉ: "% Processor Performance" của Windows × xung cơ bản. Xung chính xác cần Cảm biến nâng cao.
hw-core-p = Nhân P
hw-core-e = Nhân E
hw-kind-temperature = Nhiệt độ
hw-kind-voltage = Điện áp
hw-kind-power = Công suất
hw-kind-clock = Xung nhịp
hw-kind-load = Tải
hw-kind-fan = Quạt
hw-kind-control = Mức quạt
hw-kind-level = Mức
hw-kind-energy = Dung lượng
hw-kind-data = Dữ liệu
hw-kind-throughput = Tốc độ truyền
hw-note-needs-pawnio = Cần Cảm biến nâng cao (PawnIO) — không có nếu thiếu.
hw-note-gpu-asleep = GPU đang ngủ để tiết kiệm điện; giá trị sẽ hiện khi GPU thức dậy.
hw-note-not-reported = Driver không cung cấp thông số này.

hw-s-cpu-total = Tổng CPU
hw-s-cpu-thread-max = Luồng bận nhất
hw-s-cpu-thread = Luồng #{ $n }
hw-s-cpu-clock-avg = Xung trung bình
hw-s-mem-load = Bộ nhớ
hw-s-mem-used = Bộ nhớ đã dùng
hw-s-mem-available = Bộ nhớ còn trống
hw-s-mem-commit-load = Bộ nhớ ảo
hw-s-mem-commit-used = Bộ nhớ ảo đã dùng
hw-s-mem-commit-limit = Giới hạn bộ nhớ ảo
hw-s-acpi-zone = Vùng nhiệt { $detail }
hw-s-gpu-temp = Lõi GPU
hw-s-gpu-clock-core = Lõi GPU
hw-s-gpu-clock-mem = Bộ nhớ GPU
hw-s-gpu-power = Toàn GPU
hw-s-gpu-power-limit = Giới hạn công suất
hw-s-gpu-fan-duty = Quạt GPU
hw-s-gpu-fan = Quạt GPU
hw-s-gpu-load = Lõi GPU
hw-s-gpu-load-3d = 3D
hw-s-gpu-load-copy = Sao chép
hw-s-gpu-load-decode = Giải mã video
hw-s-gpu-load-encode = Mã hóa video
hw-s-gpu-load-compute = Tính toán
hw-s-gpu-mem-used = Bộ nhớ riêng đã dùng
hw-s-gpu-mem-shared = Bộ nhớ dùng chung đã dùng
hw-s-gpu-mem-load = Bộ nhớ riêng
hw-s-gpu-mem-total = Tổng bộ nhớ riêng
hw-s-disk-temp = Nhiệt độ
hw-s-disk-active = Thời gian hoạt động
hw-s-disk-read = Tốc độ đọc
hw-s-disk-write = Tốc độ ghi
hw-s-bat-charge = Mức pin
hw-s-bat-wear = Độ chai
hw-s-bat-rate = Tốc độ sạc / xả
hw-s-bat-voltage = Điện áp
hw-s-bat-remaining = Dung lượng còn lại
hw-s-bat-full = Dung lượng khi sạc đầy
hw-s-bat-design = Dung lượng thiết kế
hw-s-net-down = Tốc độ tải xuống
hw-s-net-up = Tốc độ tải lên

## Thẻ nhiệt độ trên Home

home-temps-title = Nhiệt độ
home-temps-open = Mở Phần cứng → Cảm biến
home-temp-cpu = CPU
home-temp-gpu = GPU
home-temp-disk = Ổ nóng nhất
home-temp-acpi = Vùng nhiệt ACPI (xấp xỉ)
home-temp-load = tải { $percent } %
home-temp-why-needs-pawnio = cần Cảm biến nâng cao
home-temp-why-gpu-asleep = GPU đang ngủ (tiết kiệm điện)
home-temp-why-not-reported = không có dữ liệu
home-temp-why-none = không khả dụng
