# Cài đặt, Dọn tự động: cảnh báo ổ sắp đầy, dọn theo lịch, khởi động cùng
# Windows (docs/SCHEDULE.md).

schedule-title = Dọn tự động
schedule-sub = Vài việc nhỏ NANOFreeDisk tự làm khi đang chạy. Mọi thứ ở đây đều tắt cho đến khi bạn bật.
schedule-loading = Đang tải…

schedule-low-heading = Ổ sắp đầy
schedule-low-toggle = Báo cho tôi khi một ổ sắp đầy
schedule-low-percent = Dưới tỉ lệ này của ổ (%)
schedule-low-gb = Hoặc còn trống dưới (GB)
schedule-low-hint = Lấy mức nhỏ hơn trong hai mức. Mỗi ổ chỉ báo một lần, nhiều nhất một lần mỗi ngày, và chỉ báo lại sau khi đã có thêm chỗ trống. Khi bật mục này, nó thay cho cảnh báo ổ sắp đầy của phần sức khỏe ổ đĩa.

schedule-clean = Tự động dọn
schedule-clean-label = Bao lâu một lần
schedule-freq-off = Tắt
schedule-freq-weekly = Hằng tuần
schedule-freq-monthly = Hằng tháng
schedule-idle = Chỉ khi máy không được dùng trong vài phút
schedule-what = Chỉ các mục An toàn mà chương trình tự tạo lại được, như tệp tạm và bộ nhớ đệm. Không bao giờ đụng tới mục cần quyền quản trị, Thùng rác, tệp trùng, Dọn gọn hay những gì bạn chọn trong Phân tích, và không đụng tới chương trình đang mở. Các mục được chuyển vào khu cách ly: bạn có thể khôi phục trong Lịch sử dọn dẹp trong 7 ngày.
schedule-categories = Nhóm được dọn
schedule-needs-running = Chỉ dọn tự động khi NANOFreeDisk đang mở hoặc đang chờ ở khay hệ thống. Hãy bật "Khởi động NANOFreeDisk cùng Windows" bên dưới, hoặc để ứng dụng luôn mở. Lần dọn bị lỡ sẽ chạy một lần, vài phút sau lần mở kế tiếp.
schedule-unsupported = Dọn tự động có trên Windows. Cảnh báo ổ sắp đầy vẫn dùng được ở đây.

schedule-autostart-heading = Khởi động cùng máy
schedule-autostart = Khởi động NANOFreeDisk cùng Windows (ở khay hệ thống)
schedule-autostart-mac = Khởi động NANOFreeDisk khi đăng nhập (ở thanh menu)
schedule-autostart-hint = Ứng dụng khởi động ẩn, chỉ hiện biểu tượng. Khi đó đóng cửa sổ thì ứng dụng vẫn chạy ở đó; chọn Thoát trong menu của biểu tượng để tắt hẳn. Trong Ứng dụng khởi động, mục này có tên NANOFreeDisk.
schedule-autostart-blocked = Mục này đang bị tắt trong Ứng dụng khởi động của Task Manager. Hãy tắt rồi bật lại ở đây để nó được khởi động.
schedule-autostart-on = NANOFreeDisk sẽ khởi động cùng Windows, ở khay hệ thống.
schedule-autostart-off = NANOFreeDisk không còn khởi động cùng Windows.

schedule-last = Lần dọn tự động gần nhất: { $date }, giải phóng { $size }
schedule-last-none = Chưa có lần dọn tự động nào.
schedule-next = Lần tới không sớm hơn { $date }
schedule-history = Lịch sử dọn dẹp
schedule-run-now = Chạy ngay
schedule-busy = Đang làm…
schedule-preview-title = Nếu dọn tự động lúc này, các mục sau sẽ được dọn
schedule-preview-total = { $count } mục, tổng cộng { $size }
schedule-preview-where = Các mục được chuyển vào khu cách ly. Bạn có thể khôi phục trong Lịch sử dọn dẹp trong 7 ngày.
schedule-preview-empty = Hiện không có gì để dọn.
schedule-preview-go = Dọn ngay các mục này
schedule-preview-cancel = Hủy
schedule-preview-expired = Bản xem trước đã quá cũ. Hãy bấm Chạy ngay lần nữa để xem những gì sẽ được dọn.
schedule-done = Đã tự động dọn { $size }. Xem trong Lịch sử dọn dẹp.

# nhóm quy tắc trong danh sách chọn
schedule-cat-ai-agent = Tác tử AI
schedule-cat-ai-app = Ứng dụng AI
schedule-cat-ai-ide = Trình soạn mã có AI
schedule-cat-browser = Trình duyệt
schedule-cat-editor = Trình soạn mã
schedule-cat-game-engine = Công cụ làm game
schedule-cat-gpu = Bộ đệm shader đồ họa
schedule-cat-ide = IDE
schedule-cat-llm-runtime = Trình chạy mô hình AI cục bộ
schedule-cat-ml-cache = Bộ đệm học máy
schedule-cat-package-manager = Trình quản lý gói
schedule-cat-sdk = SDK
schedule-cat-system = Windows
schedule-cat-tool = Công cụ
schedule-cat-toolchain = Bộ công cụ lập trình

# thông báo và menu khay hệ thống (ứng dụng điền vào)
schedule-note-auto-title = Dọn tự động
schedule-note-auto-body = Đã tự động dọn { $size }. Xem trong Lịch sử dọn dẹp.
schedule-note-low-title = { $drive } sắp đầy
schedule-note-low-body = Còn trống { $free }. Mở NANOFreeDisk từ biểu tượng ở khay hệ thống và chọn Giải phóng dung lượng.
schedule-tray-free-up = Giải phóng dung lượng

activity-kind-clean-scheduled = Dọn tự động
