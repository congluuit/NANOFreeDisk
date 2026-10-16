# Monitor — Hoạt động ổ đĩa (docs/PHASE6_MONITOR.md). Chỉ đọc: không có gì ở đây
# kết thúc hay thay đổi một tiến trình.

mon-title = Hoạt động ổ đĩa
mon-subtitle = Cái gì đang đọc và ghi ổ đĩa ngay lúc này, tới từng tệp khi bật chi tiết. Chỉ đọc — không dừng hay thay đổi gì.
mon-tab-activity = Trực tiếp
mon-tab-history = 24 giờ
mon-tab-who = Ai đã chạm vào đây?

mon-source-counters = Bộ đếm I/O
mon-source-etw = Chi tiết theo tệp
mon-source-helper = Chi tiết theo tệp (helper admin)
mon-source-counters-hint = Bộ đếm I/O của từng tiến trình: mọi lần đọc và ghi, kể cả mạng và pipe, không có tên tệp. Không cần quyền admin.
mon-source-etw-hint = Một phiên trace kernel trong app (app đang chạy quyền admin): đọc và ghi theo tiến trình và theo tệp trên ổ cục bộ.
mon-source-helper-hint = Một phiên trace kernel trong helper chạy quyền admin: đọc và ghi theo tiến trình và theo tệp trên ổ cục bộ.

mon-detail-on = Xem tệp (admin)
mon-detail-off = Tắt chi tiết tệp
mon-detail-declined = Chi tiết theo tệp cần quyền quản trị; Windows đã bị từ chối.
mon-detail-failed = Không bật được chi tiết theo tệp: { $message }
mon-detail-error-etw-silent = Phiên trace không báo hoạt động tệp nào trong khi ổ đang bận (phần mềm bảo mật có thể đang chặn), nên Monitor quay về bộ đếm I/O.
mon-detail-error-helper-stopped = Helper admin đã dừng. Bật lại chi tiết theo tệp để khởi động lại.
mon-detail-error-session-stopped = Phiên trace bị dừng từ bên ngoài app. Bật lại chi tiết theo tệp để khởi động lại.
mon-detail-error-other = Chi tiết theo tệp đã dừng. Bật lại để thử lần nữa.
mon-pause = Tạm dừng
mon-resume = Tiếp tục
mon-paused-note = Monitor đang tạm dừng: không lấy mẫu, không có phiên trace nào chạy.
mon-error = Không đọc được hoạt động ổ đĩa: { $message }
mon-loading = Đang đo…

mon-read = Đọc
mon-write = Ghi
mon-r = Đ
mon-w = G
mon-ops = thao tác/s
mon-window = Khoảng thời gian
mon-window-60 = 60 giây
mon-window-600 = 10 phút
mon-ago-60 = 60 giây trước
mon-ago-600 = 10 phút trước
mon-chart-label = Tốc độ đọc và ghi của mọi tiến trình
mon-disk = Ổ { $index }
mon-queue = Hàng đợi
mon-latency = Độ trễ
mon-busy = Bận { $pct } %
mon-no-disks = Không có bộ đếm ổ vật lý.

mon-col-name = Tiến trình
mon-col-pid = PID
mon-col-read = Đọc/s
mon-col-write = Ghi/s
mon-col-iops = IOPS
mon-col-files = Tệp
mon-idle = Không tiến trình nào đang đọc hay ghi.
mon-reveal = Mở trong Explorer
mon-reveal-exe = Mở chương trình trong Explorer
mon-files-need-detail = Bật chi tiết theo tệp để xem tiến trình này dùng những tệp nào.
mon-no-files = Không có hoạt động tệp trong giây vừa qua.
mon-process-gone = { $name } đã thoát.

mon-top-written = Tệp đang bị ghi
mon-top-need-detail = Bộ đếm I/O chỉ biết tiến trình, không biết tệp. Bật chi tiết theo tệp (admin) để xem tệp đang bị ghi và mở thư mục của chúng.
mon-no-writes = Không có gì đang được ghi.
mon-lost = Phiên trace: mất { $lost } sự kiện, { $undecoded } không giải mã được.

mon-history-title = 24 giờ qua, theo giờ
mon-history-empty = Chưa ghi lại gì. Hoạt động được lưu theo phút khi view này mở hoặc cảnh báo log spam bật, và chỉ khi bật lịch sử quét trong Cài đặt.
mon-history-hint = Giữ 24 giờ trên máy này; xóa cùng lịch sử quét trong Cài đặt.
mon-top-writers = Ghi nhiều nhất

mon-who-title = Ai đã chạm vào đây?
mon-who-hint = Chọn một thư mục để xem tiến trình nào tạo, ghi, đổi tên và xóa tệp trong đó. Cần chi tiết theo tệp.
mon-who-placeholder = Thư mục cần theo dõi, vd. C:\Users\ban\Documents
mon-who-pick = Chọn thư mục để theo dõi
mon-who-start = Theo dõi
mon-who-stop = Ngừng theo dõi
mon-who-need-detail = Chi tiết theo tệp đang tắt: bật lên để ghi lại sự kiện.
mon-who-idle = Chưa theo dõi thư mục nào.
mon-who-none = Chưa có gì xảy ra trong thư mục này.
mon-op-create = Tạo
mon-op-write = Ghi
mon-op-delete = Xóa
mon-op-rename = Đổi tên

mon-alert-spam-title = { $name } ghi ổ liên tục
mon-alert-spam-body = { $name } đã ghi { $rate } MB/s suốt { $minutes } phút. Mở Hoạt động ổ đĩa để xem các tệp.
mon-alert-log-title = Tệp log lớn: { $name }
mon-alert-log-body = { $name } đang ghi vào { $path } ({ $size } GB).
mon-alert-spam-toast = { $name } đã ghi { $rate } suốt { $minutes } phút
mon-alert-log-toast = { $name } đang ghi vào tệp log { $size }: { $path }

hint-mon-detail-on = Bắt đầu một phiên trace kernel trong helper admin (Windows hỏi quyền một lần; dùng chung với Cảm biến nâng cao). Không thay đổi gì trên máy; phiên dừng khi bạn tạm dừng, rời view này hoặc đóng app.
hint-mon-detail-on-admin = Bắt đầu một phiên trace kernel trong app (app đang chạy quyền admin). Không thay đổi gì trên máy; phiên dừng khi bạn tạm dừng, rời view này hoặc đóng app.
hint-mon-detail-off = Dừng phiên trace; view quay về bộ đếm I/O.
hint-mon-pause = Dừng mọi lần lấy mẫu và phiên trace cho tới khi bạn tiếp tục. Cảnh báo cũng dừng.
hint-mon-resume = Lấy mẫu lại (và chi tiết theo tệp, nếu trước đó đang bật).
hint-mon-end-task = Mở hộp thoại Kết thúc tác vụ của Phần cứng → Tasks: kiểm tra tiến trình trước, hỏi bạn xác nhận và không bao giờ kết thúc tiến trình hệ thống.
hint-mon-analyze = Quét thư mục chứa tệp này trong Phân tích (chỉ đọc).
hint-mon-watch = Ghi lại ai tạo, ghi, đổi tên và xóa tệp trong thư mục này khi chi tiết theo tệp đang bật. Không thay đổi gì.
hint-mon-unwatch = Ngừng ghi lại sự kiện của thư mục này.

settings-monitor = Hoạt động ổ đĩa
settings-mon-alerts = Cảnh báo khi một tiến trình ghi ổ liên tục
settings-mon-mbps = Nhanh hơn (MB/s)
settings-mon-minutes = Trong (phút)
settings-mon-hint = Bắt các trình ghi log mất kiểm soát, indexer và app đồng bộ. Khi bật, Monitor lấy mẫu mỗi giây ở nền (dưới 1 % CPU); tệp log trên 1 GB đang bị ghi cũng được báo.
hint-mon-alerts = Khi bật, app lấy mẫu hoạt động ổ đĩa mỗi giây ở nền và hiện thông báo khi một tiến trình ghi nhanh hơn ngưỡng suốt cả khoảng thời gian. Không bao giờ dừng tiến trình đó.
hint-mon-threshold = Đổi ngưỡng cảnh báo log spam.

tray-open = Mở NANOFreeDisk
tray-pause = Tạm dừng theo dõi ổ đĩa
tray-quit = Thoát
