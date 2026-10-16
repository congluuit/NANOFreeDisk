## Health view (Phase 7, docs/PHASE7_HEALTH.md)

health-title-view = Sức khỏe ổ đĩa
health-subtitle = Đánh giá Tốt / Cần chú ý / Xấu cho từng ổ từ dữ liệu S.M.A.R.T. hoặc NVMe của chính ổ, theo cách CrystalDiskInfo đánh giá, kèm các kiểm tra chỉ-đọc cho từng phân vùng. Ở đây không có gì thay đổi ổ đĩa.
health-refresh = Đọc lại các ổ
health-elevate = Đọc đầy đủ (quyền quản trị)
health-elevate-hint = Nếu bấm: Windows hỏi quyền quản trị một lần; một tiến trình phụ đọc ngưỡng S.M.A.R.T., ổ cắm qua bộ chuyển USB và cờ cần kiểm tra đĩa của phân vùng rồi tự đóng. Chỉ đọc — không ghi gì vào ổ nào.
health-elevated-done = Đã đọc các ổ bằng quyền quản trị.
health-elevated-declined = Không được cấp quyền quản trị; đang hiện những gì người dùng thường đọc được.
health-export-html = Lưu báo cáo
health-export-html-hint = Nếu bấm: lưu báo cáo HTML tự chứa về các ổ và phân vùng (không có số serial trừ khi bạn chọn) vào tệp bạn chọn.
health-export-json = Xuất JSON
health-export-json-hint = Nếu bấm: lưu báo cáo dạng JSON (nanofreedisk.health.v1) vào tệp bạn chọn.
health-export-title = Lưu báo cáo sức khỏe ổ đĩa
health-exported = Đã lưu vào { $path }
health-serials = Kèm số serial
health-serials-hint = Nếu chọn: báo cáo xuất ra có số serial của từng ổ (định danh phần cứng của bạn — hãy tắt khi gửi cho người khác).
health-error = Không đọc được các ổ: { $message }
health-loading = Đang đọc các ổ…
health-standard-user = Đọc với quyền người dùng thường: ngưỡng S.M.A.R.T., ổ cắm qua bộ chuyển USB và cờ cần kiểm tra đĩa cần quyền quản trị.
health-no-drives = Không liệt kê được ổ vật lý nào.
health-drives = Ổ đĩa
health-volumes = Phân vùng
health-no-problems = Không thấy vấn đề nào.
health-no-thresholds = Chưa đọc được ngưỡng hỏng (cần quyền quản trị), nên chỉ đánh giá theo các bộ đếm thô.

health-status-unknown = Không rõ
health-status-good = Tốt
health-status-caution = Cần chú ý
health-status-bad = Xấu
health-ssd = SSD
health-hdd = Ổ cứng HDD

health-source-nvme = Nhật ký sức khỏe NVMe
health-source-ata-driver = S.M.A.R.T. (qua driver lưu trữ)
health-source-ata-smart = S.M.A.R.T. kèm ngưỡng
health-source-sat = S.M.A.R.T. qua bộ chuyển USB
health-source-none = Không có dữ liệu sức khỏe

health-note-usb-bridge = Bộ chuyển USB này không chuyển tiếp S.M.A.R.T. (hoặc cần quyền quản trị để thử). Không có nghĩa là ổ bị hỏng.
health-note-needs-admin = Ngưỡng và S.M.A.R.T. đầy đủ cần quyền quản trị — dùng "Đọc đầy đủ".
health-note-raid = RAID / Storage Spaces: không đọc được các ổ thành viên qua mảng.
health-note-virtual = Ổ ảo: không có dữ liệu sức khỏe riêng.
health-note-no-smart = Ổ này không báo dữ liệu S.M.A.R.T. hay NVMe.

health-reason-nvme-spare-low = Vùng dự phòng chỉ còn { $value } %, dưới ngưỡng của chính ổ — hãy sao lưu và tính đến việc thay ổ.
health-reason-nvme-reliability = Ổ báo độ tin cậy đã giảm (lỗi vùng nhớ hoặc lỗi bên trong) — hãy sao lưu ngay.
health-reason-nvme-read-only = Ổ đã tự chuyển sang chế độ chỉ đọc — hãy chép dữ liệu ra khỏi ổ.
health-reason-nvme-backup = Bộ lưu dự phòng khi mất điện của ổ đã hỏng.
health-reason-nvme-temperature = Ổ báo cảnh báo nhiệt độ.
health-reason-nvme-media-errors = Đã ghi nhận { $value } lỗi vùng nhớ / toàn vẹn dữ liệu.
health-reason-worn = Ổ chỉ còn { $value } % tuổi thọ định mức.
health-reason-ata-predict-failure = Chính ổ dự báo sắp hỏng — hãy sao lưu ngay.
health-reason-ata-threshold = Thuộc tính { $attr } { $name } đã xuống dưới ngưỡng hỏng (hiện { $value }).
health-reason-ata-reallocated = { $value } sector hỏng đã được thay bằng sector dự phòng.
health-reason-ata-pending = { $value } sector không đọc được, đang chờ được thay.
health-reason-ata-uncorrectable = { $value } sector không khôi phục được.

health-temperature = Nhiệt độ
health-temp-limits = (cảnh báo ở { $warning } °C, nguy hiểm ở { $critical } °C)
health-life = Tuổi thọ còn lại
health-power-on = Thời gian chạy
health-hours = { $hours } giờ ({ $days } ngày)
health-minutes = { $n } phút
health-power-cycles = Số lần bật
health-written = Dữ liệu đã ghi
health-read = Dữ liệu đã đọc
health-firmware = Firmware
health-serial = Số serial
health-trim-support = Hỗ trợ TRIM
health-yes = Có
health-no = Không
health-none = Không có
health-not-checked = Chưa kiểm tra (cần quyền quản trị)

health-nvme-warning = Cảnh báo nghiêm trọng
health-nvme-spare = Vùng dự phòng còn
health-nvme-spare-value = { $spare } % (ngưỡng { $threshold } %)
health-nvme-used = Mức đã dùng
health-nvme-media-errors = Lỗi vùng nhớ
health-nvme-unsafe = Số lần tắt không an toàn
health-nvme-read = Dữ liệu đã đọc
health-nvme-written = Dữ liệu đã ghi
health-nvme-host-reads = Lệnh đọc từ máy
health-nvme-host-writes = Lệnh ghi từ máy
health-nvme-busy = Thời gian controller bận
health-nvme-error-log = Mục nhật ký lỗi
health-nvme-warn-time = Thời gian trên ngưỡng cảnh báo nhiệt
health-nvme-crit-time = Thời gian trên ngưỡng nguy hiểm
health-nvme-sensors = Cảm biến nhiệt độ

health-attr-name = Thuộc tính
health-attr-current = Hiện tại
health-attr-worst = Thấp nhất
health-attr-threshold = Ngưỡng
health-attr-raw = Giá trị thô
health-attr-vendor = Riêng của hãng

health-vol-free = Dung lượng trống
health-vol-forecast = Đầy sau
health-vol-full-in = khoảng { $days } ngày với tốc độ hiện tại
health-vol-not-filling = không đầy thêm
health-vol-no-trend = chưa đủ lịch sử quét
health-vol-fs = Hệ thống tệp
health-vol-cluster = Kích thước cluster
health-vol-aligned = Phân vùng căn lề (4 KB)
health-vol-trim = Windows gửi TRIM
health-vol-bitlocker = BitLocker
health-vol-dirty = Cần kiểm tra đĩa
health-bitlocker-on = Bật
health-bitlocker-off = Tắt
health-bitlocker-encrypting = Đang mã hóa
health-bitlocker-decrypting = Đang giải mã
health-bitlocker-suspended = Tạm dừng
health-bitlocker-locked = Đang khóa
health-bitlocker-waiting = Chờ kích hoạt

health-col-bus = Kết nối
health-col-size = Dung lượng
health-report-title = Báo cáo sức khỏe ổ đĩa
health-report-standard-user = đọc với quyền người dùng thường
health-report-footer = Tạo bằng NANOFreeDisk — chẩn đoán chỉ đọc; không có gì trên các ổ này bị thay đổi.

## Health Score findings (Home)

health-drive-bad = { $folder } đang hỏng dần — hãy sao lưu dữ liệu ngay.
health-drive-caution = { $folder } cần chú ý (sức khỏe S.M.A.R.T. / NVMe) — hãy giữ một bản sao lưu.
health-drive-hot = { $folder } đang ở { $value } °C, bằng hoặc trên ngưỡng nguy hiểm của chính ổ.
health-drive-warm = { $folder } đang ở { $value } °C, trên ngưỡng cảnh báo của chính ổ.
health-drive-unknown = Không đọc được sức khỏe của { $folder }.
health-volume-dirty = { $drive } bị đánh dấu cần kiểm tra đĩa (chkdsk) — Windows sẽ kiểm tra ở lần khởi động lại tới.
health-fix-health = Xem sức khỏe ổ đĩa
