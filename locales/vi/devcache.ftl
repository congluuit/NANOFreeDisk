# View DevCache (Phase 4, docs/PHASE4_DEVCACHE.md). Nạp cùng app.ftl và clean.ftl;
# mọi key phải có trong en/devcache.ftl.

dev-title = Cache lập trình
dev-intro = Cache của công cụ AI, IDE, trình quản lý gói và game engine theo vendor, các toolchain đã cài, build output trong repo và các thư mục cache chưa có quy tắc. Khi dọn, mục được chuyển vào quarantine: khôi phục được từ Hoạt động trong 7 ngày.
dev-tab-cache = Cache
dev-tab-toolchains = Toolchain
dev-tab-repos = Repo
dev-tab-stores = Package store
dev-tab-unknown = Thư mục lạ
dev-scanning = Đang tìm cache lập trình… (chỉ đọc)
dev-nothing = Không tìm thấy cache lập trình nào.
dev-kept-count = { $count } mục cố tình giữ lại
dev-kept = Giữ lại:
dev-kept-hint = Cấu hình, đăng nhập, extension hoặc dữ liệu của bạn: NANOFreeDisk không bao giờ dọn phần này.
dev-safe-all-hint = Nếu bấm: mọi cache lập trình mức An toàn của mọi vendor được chuyển vào quarantine. Ứng dụng tự tạo lại; bạn khôi phục được từ Hoạt động trong 7 ngày.
dev-safe-part = an toàn { $size }
dev-safe-vendor = Dọn an toàn ({ $size })
dev-safe-vendor-hint = Nếu bấm: các cache An toàn của { $vendor } được chuyển vào quarantine (khôi phục được từ Hoạt động trong 7 ngày).
dev-over = vượt { $size }
dev-over-hint = Lớn hơn ngưỡng nhắc đặt trong Cài đặt → Cache lập trình.
dev-over-title = Cache lập trình đang phình to
dev-over-body = Vượt ngưỡng: { $list }. Mở NANOFreeDisk → Cache lập trình để xem.
dev-over-toast = Cache lập trình vượt ngưỡng: { $list }
dev-last-write = sửa lần cuối { $date }
dev-running = { $apps } đang chạy — hãy đóng trước, nếu không các tệp nó đang mở sẽ được bỏ qua.
dev-needs-closed = Cần đóng { $apps } trước
dev-open-items = Xem thư mục
dev-reveal = Mở trong Explorer

## Toolchain (C2)
dev-tools-note = SDK, runtime và editor đã cài kèm cách gỡ đúng cho từng cái. NANOFreeDisk không bao giờ xóa thư mục toolchain: hãy sao chép lệnh hoặc dùng ứng dụng được nêu.
dev-tools-loading = Đang đo toolchain… (chỉ đọc)
dev-tools-none = Không tìm thấy toolchain nào.
dev-tool-default = mặc định
dev-tool-default-hint = Phiên bản mặc định / đang dùng
dev-guide-windows-apps = Gỡ trong Cài đặt → Ứng dụng → Ứng dụng đã cài
dev-guide-unity-hub = Gỡ trong Unity Hub → Installs
dev-guide-epic-launcher = Gỡ trong Epic Games Launcher → Unreal Engine → Library
dev-guide-visual-studio-installer = Sửa hoặc gỡ trong Visual Studio Installer
dev-guide-ide = Gỡ trong IDE hoặc công cụ đã tải nó
dev-guide-compact-vhd = Thu nhỏ: giải phóng dung lượng bên trong trước, rồi wsl --shutdown và Optimize-VHD -Mode Full

## Repo (C3, C9)
dev-repos-root = Thư mục chứa dự án của bạn
dev-repos-pick = Thư mục để tìm repo
dev-repos-choose = Chọn…
dev-repos-scan = Tìm repo
dev-repos-scan-hint = Nếu bấm: thư mục được tìm (chỉ đọc) các git repo và dự án, rồi đo build output của chúng.
dev-repos-policy = Build output được nhận diện theo tệp dự án (Cargo.toml → target, package.json → node_modules…). Tên chung chung như build hay dist chỉ được đề xuất khi git ignore chúng, thư mục có tệp được git theo dõi thì không bao giờ, và dự án hoạt động trong { $days } ngày gần nhất không được chọn sẵn.
dev-repos-loading = Đang tìm repo… (chỉ đọc)
dev-repos-summary = { $count } dự án trong { $folders } thư mục
dev-repos-no-git = Không tìm thấy git: các thư mục chung chung (build, dist, bin, obj) sẽ không được đề xuất.
dev-repos-none = Không tìm thấy repo hay dự án nào ở đây.
dev-repo-no-git = không có git
dev-repo-dirty = có thay đổi chưa commit
dev-repo-dirty-hint = git status có thay đổi chưa commit (build output không bị ảnh hưởng)
dev-repo-recent = mới hoạt động
dev-repo-recent-hint = Có dùng trong { $days } ngày gần nhất: build output không được chọn sẵn
dev-repo-stale = lâu không dùng
dev-repo-commit = commit cuối { $date }:
dev-repo-activity = hoạt động cuối { $date }
dev-gc = Chỉ hướng dẫn:
dev-gc-hint = git gc nén repo; không hoàn tác được nên NANOFreeDisk chỉ hiện lệnh.
dev-art-tracked = Không đề xuất: git đang theo dõi tệp trong đó
dev-art-not-ignored = Không đề xuất: git không ignore nó (hoặc không có git)
dev-art-impact-safe = Build output: lần build sau sẽ tạo lại.
dev-art-impact-caution = Dựng lại mất nhiều thời gian (import lại, biên dịch shader hoặc build lớn), hoặc phải tạo lại môi trường.
dev-kind-rust = Rust
dev-kind-node = Node.js
dev-kind-python = Python
dev-kind-dotnet = .NET
dev-kind-cmake = CMake
dev-kind-gradle = Gradle
dev-kind-maven = Maven
dev-kind-flutter = Flutter / Dart
dev-kind-unity = Unity
dev-kind-unreal = Unreal Engine
dev-kind-godot = Godot
dev-kind-terraform = Terraform
dev-kind-ruby = Ruby
dev-repos-clean-hint = Nếu bấm: bạn xem danh sách trước, rồi build output đã chọn được chuyển vào quarantine (khôi phục được từ Hoạt động trong 7 ngày).
dev-repos-confirm-title = Chuyển build output này vào quarantine?
dev-repos-confirm-body = { $count } thư mục, { $size }. Lần build sau sẽ tạo lại chúng.

## Package store (C4)
dev-stores-note = Lệnh dọn chính chủ của từng trình quản lý gói: nó biết phần nào còn đang dùng nên không làm hỏng dự án. Lệnh chỉ chạy sau khi bạn xem đúng dòng lệnh; lệnh không hoàn tác được thì chỉ hướng dẫn.
dev-stores-loading = Đang tìm trình quản lý gói…
dev-stores-none = Không tìm thấy trình quản lý gói nào có lệnh dọn.
dev-undo-redownload = Hoàn tác: công cụ tự tải lại khi cần
dev-undo-none = Không hoàn tác được — chỉ hướng dẫn
dev-store-missing = Chương trình không có trong PATH; chỉ đo thư mục.
dev-store-run = Chạy
dev-store-run-hint = Nếu bấm: hộp thoại hiện đúng dòng lệnh sẽ chạy; không có gì chạy cho tới khi bạn xác nhận.
dev-store-guide = Chỉ hướng dẫn
dev-store-guide-hint = Thứ lệnh này xóa không lấy lại được nên NANOFreeDisk không bao giờ chạy nó: sao chép và tự chạy nếu bạn chắc chắn.
dev-store-confirm-title = Chạy lệnh dọn của { $tool }?
dev-store-confirm-body = Store đang chiếm { $size }. Đúng lệnh này sẽ được chạy:
dev-store-confirm-note = Lệnh chạy không qua shell, không nhận input, dừng sau 10 phút và được ghi vào Hoạt động (không khôi phục được: công cụ tự tải lại những gì cần).
dev-store-confirm-go = Chạy lệnh
dev-store-running = Đang chạy { $id }…
dev-store-done = Xong — đã giải phóng { $size }
dev-store-failed = Lệnh không hoàn thành thành công.
dev-store-recorded = Đã ghi vào Hoạt động.
dev-store-output = Kết quả lệnh

## Thư mục lạ
dev-unknown-note = Các thư mục trông như cache nhưng chưa có quy tắc mô tả. Chúng không bao giờ được dọn tự động — hãy tự kiểm tra hoặc đề xuất quy tắc.
dev-unknown-loading = Đang tìm cache lạ… (chỉ đọc)
dev-unknown-none = Không có thư mục cache lạ nào từ 50 MB trở lên.
dev-unknown-count = { $count } thư mục lạ
dev-unverified = Chưa xác minh
dev-unknown-why = Trông như cache: { $signals }
dev-unknown-dot = thư mục ẩn của công cụ
dev-propose = Đề xuất quy tắc
dev-propose-hint = Nếu bấm: GitHub mở đề xuất quy tắc điền sẵn, thư mục được ghi dạng biến (không có tên người dùng) kèm kích thước.

## Cài đặt
settings-devcache = Cache lập trình
settings-dev-alerts = Nhắc khi cache của một vendor vượt ngưỡng
hint-dev-alerts = Nếu bật: lần quét Cache lập trình (và một lần kiểm tra sau khi mở app một phút) hiện thông báo cho vendor vượt ngưỡng.
settings-dev-thresholds-hint = Ngưỡng nhắc theo vendor (0 = không nhắc).
settings-dev-threshold = Ngưỡng cho { $vendor }
hint-dev-threshold = Nếu đổi: vendor được đánh dấu và bạn được nhắc khi cache của nó vượt kích thước này.
settings-dev-remove = Bỏ ngưỡng
settings-dev-vendor = Vendor (ví dụ Gradle)
settings-dev-add = Thêm
settings-dev-keep-recent = Giữ build output của dự án hoạt động trong (ngày)
hint-dev-keep-recent = Nếu đổi: build output của dự án dùng gần đây vẫn được liệt kê nhưng không chọn sẵn (0 = tắt).
settings-dev-stale = Đánh dấu dự án không hoạt động sau (ngày)
hint-dev-stale = Nếu đổi: repo không dùng lâu như vậy được đánh dấu lâu không dùng.
activity-kind-devcache = Đã dọn cache lập trình
activity-kind-command = Lệnh dọn kho gói
