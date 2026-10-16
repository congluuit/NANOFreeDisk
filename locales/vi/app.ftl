# NANOFreeDisk desktop app — Tiếng Việt.
# Mọi key ở đây phải có trong locales/en/app.ftl (kiểm tra bởi scripts/check-locales.mjs).

app-name = NANOFreeDisk
app-tagline = Biết cái gì chiếm ổ đĩa. Giải phóng an toàn.

## Navigation

nav-home = Trang chủ
nav-analyze = Phân tích
nav-clean = Dọn dẹp
nav-devcache = Cache lập trình
nav-duplicates = Tệp trùng lặp
nav-monitor = Hoạt động ổ đĩa
nav-health = Sức khỏe ổ đĩa
nav-settings = Cài đặt
nav-soon = Có trong { $phase }

## Home

home-title = Ổ đĩa của bạn
home-subtitle = Chọn ổ đĩa hoặc thư mục để xem cái gì chiếm dung lượng. Không có gì bị xóa nếu bạn chưa xác nhận.
home-scan-folder = Quét một thư mục…
home-pick-folder = Chọn thư mục cần phân tích
home-action-free = Giải phóng dung lượng an toàn
home-action-free-hint = Dọn rác & cache sẽ có cùng bộ máy an toàn (Phase 3).
home-action-analyze = Xem cái gì chiếm ổ đĩa
home-action-analyze-hint = Treemap, sunburst và danh sách thư mục, tệp lớn nhất.
home-action-health = Ổ đĩa có khỏe không?
home-action-health-hint = Sức khỏe S.M.A.R.T. và NVMe sẽ có ở Phase 7.
home-refresh = Làm mới
home-no-drives = Không tìm thấy ổ đĩa nào.

drive-used = Đã dùng { $used } / { $total }
drive-free = Còn trống { $free }
drive-system = Hệ thống
drive-not-ready = Không có đĩa
drive-scan = Phân tích
drive-kind-fixed = Ổ cục bộ
drive-kind-removable = Ổ rời
drive-kind-network = Ổ mạng
drive-kind-optical = Ổ quang
drive-kind-ramdisk = Ổ RAM
drive-kind-unknown = Ổ đĩa

## Scanning

scan-title = Đang quét { $path }
scan-files = { $count } tệp
scan-dirs = { $count } thư mục
scan-size = Đã thấy { $size }
scan-errors = { $count } không đọc được
scan-elapsed = { $time }
stat-files = Tệp
stat-dirs = Thư mục
stat-size = Dung lượng
stat-time = Thời gian
scan-cancel = Hủy
scan-cancelled = Đã hủy quét.
scan-failed = Quét thất bại: { $error }
scan-hint = Không bao giờ đi theo liên kết, junction hay ổ khác; tệp hardlink chỉ được tính một lần.

## Analyze

analyze-empty = Chưa có kết quả quét. Hãy phân tích một ổ đĩa hoặc thư mục từ Trang chủ.
analyze-up = Lên một cấp
analyze-rescan = Quét lại
analyze-export = Xuất
analyze-export-json = Báo cáo (JSON)
analyze-export-csv = Toàn bộ mục (CSV)
analyze-exported = Đã lưu { $path }
analyze-summary = { $size } trong { $files } tệp · { $dirs } thư mục · quét trong { $time }
analyze-issues = { $count ->
    [one] 1 đường dẫn không đọc được
   *[other] { $count } đường dẫn không đọc được
}
analyze-of-drive = { $percent } ổ đĩa
analyze-search-placeholder = Tìm theo tên (vd. *.iso, node_modules)…
analyze-view-treemap = Treemap
analyze-view-sunburst = Sunburst
analyze-tab-top = Tệp lớn nhất
analyze-tab-ext = Loại tệp
analyze-tab-search = Kết quả tìm kiếm
analyze-tab-issues = Không đọc được
analyze-truncated = Chỉ hiển thị các mục lớn nhất; phóng to để xem chi tiết hơn.
analyze-small-items = Mục nhỏ
analyze-legend-folder = Thư mục

col-name = Tên
col-size = Dung lượng
col-logical = Kích thước
col-share = Tỷ lệ
col-files = Số tệp
col-modified = Sửa đổi
col-path = Đường dẫn
col-type = Loại
col-reason = Lý do

kind-file = Tệp
kind-dir = Thư mục
kind-symlink = Liên kết tượng trưng
kind-junction = Junction
kind-mount_point = Điểm gắn ổ
kind-other = Khác

attr-system = Hệ thống
attr-hidden = Ẩn
attr-reparse = Reparse point
attr-compressed = Đã nén
attr-sparse = Sparse
attr-offline = Chỉ trên cloud
attr-readonly = Chỉ đọc
attr-hardlink = Hardlink (chỉ tính một lần)

cat-video = Video
cat-image = Hình ảnh
cat-audio = Âm thanh
cat-archive = Tệp nén & bộ cài
cat-code = Mã nguồn & văn bản
cat-binary = Chương trình & thư viện
cat-document = Tài liệu
cat-data = Dữ liệu, log & ảnh đĩa
cat-other = Khác

ext-none = (không có phần mở rộng)

## Filters

filter-title = Bộ lọc
filter-kind = Hiển thị
filter-kind-any = Tệp & thư mục
filter-kind-files = Chỉ tệp
filter-kind-dirs = Chỉ thư mục
filter-min-size = Tối thiểu
filter-max-size = Tối đa
filter-exts = Phần mở rộng
filter-exts-placeholder = mp4, iso, zip
filter-modified = Sửa đổi
filter-modified-any = Mọi lúc
filter-modified-older-1y = Cũ hơn 1 năm
filter-modified-older-3y = Cũ hơn 3 năm
filter-modified-last-30d = 30 ngày gần đây
filter-hide-hidden = Ẩn tệp ẩn & hệ thống
filter-cloud-only = Tệp chỉ trên cloud
filter-apply = Tìm
filter-clear = Xóa lọc
filter-results = { $count } kết quả · { $size }
filter-invalid-size = Nhập kích thước dạng 500MB hoặc 2GB.

## Details & actions

details-none = Chọn một mục để xem chi tiết.
details-path = Vị trí
details-contains = Chứa
details-share = Tỷ lệ trong lần quét
details-attrs = Thuộc tính
action-open = Mở
action-reveal = Mở trong Explorer
action-zoom = Phóng to
action-copy-path = Sao chép đường dẫn
action-recycle = Chuyển vào Thùng rác
action-recycle-hint = Nếu bấm: mục sẽ được chuyển vào Thùng rác và có thể khôi phục từ đó. Các vị trí được bảo vệ của Windows, chương trình và thông tin đăng nhập sẽ bị từ chối.
action-recycle-n = Chuyển { $count } mục vào Thùng rác
action-select-hint = Ctrl+click để chọn nhiều mục.
copied = Đã sao chép

## Delete dialog

delete-title = Chuyển vào Thùng rác?
delete-intro = Các mục sau sẽ được chuyển vào Thùng rác. Bạn có thể khôi phục chúng cho tới khi dọn Thùng rác.
delete-total = { $size } trong { $files } tệp
delete-blocked-title = Sẽ không bị xóa
delete-extra = Đây là lượng dữ liệu lớn ({ $size }, { $percent } ổ đĩa). Tôi đã kiểm tra danh sách và muốn tiếp tục.
delete-confirm = Chuyển vào Thùng rác
delete-cancel = Hủy
delete-nothing = Không có mục nào ở đây được phép xóa.
delete-nuke-note = Nếu một mục quá lớn so với Thùng rác, Windows sẽ hỏi trước khi xóa vĩnh viễn.
delete-working = Đang chuyển vào Thùng rác…
delete-result = Đã chuyển { $done } mục ({ $size }) · { $failed } mục không chuyển
delete-declined = Giữ lại (đã từ chối xóa vĩnh viễn)

reason-volume-root = Không thể xóa thư mục gốc của ổ đĩa.
reason-system-dir = Nằm trong thư mục Windows — hãy dùng Dọn dẹp (Phase 3) hoặc công cụ của Windows.
reason-protected = Vị trí được bảo vệ ({ $template }): Windows, chương trình hoặc thông tin đăng nhập cần nó.
reason-contains-protected = Chứa thư mục được bảo vệ ({ $template }); hãy xóa từng mục bên trong.
reason-link = Liên kết, junction và điểm gắn ổ được gỡ trong Explorer, không phải ở đây.
reason-contains-mount-point = Chứa một ổ đĩa khác được gắn dưới dạng thư mục.
reason-unresolvable = Mục không còn tồn tại hoặc không truy cập được.

## Settings

settings-title = Cài đặt
settings-language = Ngôn ngữ
settings-language-system = Theo Windows
settings-theme = Giao diện
settings-theme-light = Sáng
settings-theme-dark = Tối
settings-theme-system = Hệ thống
settings-theme-custom = Tùy chỉnh
settings-theme-system-hint = Theo chế độ sáng/tối và màu nhấn của Windows.
settings-preset = Bắt đầu từ mẫu
settings-custom-colors = Màu sắc
settings-contrast-warning = Tương phản thấp: { $pair } chỉ { $ratio }:1 (WCAG AA cần 4.5:1).
settings-theme-import = Nhập theme.toml
settings-theme-export = Xuất theme.toml
settings-theme-reset = Đặt lại màu
settings-theme-imported = Đã nhập giao diện.
settings-theme-invalid = Tệp giao diện không hợp lệ.
settings-font-scale = Cỡ chữ
settings-about = Giới thiệu
settings-about-text = NANOFreeDisk { $version } · mã nguồn mở (MIT) · không thu thập dữ liệu.
settings-author = Phát triển bởi Lưu Đức Thành Công (NANO)

token-bg = Nền
token-surface = Bề mặt
token-surface-2 = Bề mặt nổi
token-border = Viền
token-text = Chữ
token-text-muted = Chữ phụ
token-accent = Màu nhấn
token-accent-contrast = Chữ trên màu nhấn
token-danger = Nguy hiểm
token-risk-safe = Rủi ro: an toàn
token-risk-caution = Rủi ro: cẩn thận
token-risk-advanced = Rủi ro: nâng cao
token-risk-protected = Rủi ro: được bảo vệ
token-tm-1 = Treemap: video
token-tm-2 = Treemap: hình ảnh
token-tm-3 = Treemap: âm thanh
token-tm-4 = Treemap: tệp nén
token-tm-5 = Treemap: mã nguồn
token-tm-6 = Treemap: chương trình
token-tm-7 = Treemap: tài liệu
token-tm-8 = Treemap: dữ liệu
token-tm-other = Treemap: khác
token-tm-dir = Treemap: khung thư mục

## Bảng lệnh (Ctrl+K)

palette-open = Tìm mọi thứ…
palette-title = Bảng lệnh (Ctrl+K)
palette-placeholder = Gõ lệnh, ổ đĩa, thư mục hoặc cài đặt…
palette-nothing = Không có kết quả.
palette-move = di chuyển
palette-run = chạy
palette-folders-hint = Gõ từ 2 ký tự để tìm thư mục trong lần quét này.
palette-group-go = Đi tới
palette-group-action = Hành động
palette-group-drive = Ổ đĩa
palette-group-folder = Thư mục trong lần quét
palette-group-setting = Cài đặt
palette-mode-simple = Chuyển sang chế độ Đơn giản
palette-mode-expert = Chuyển sang chế độ Chuyên gia
palette-scan-drive = Phân tích { $drive }
palette-font-larger = Chữ to hơn
palette-font-smaller = Chữ nhỏ hơn

## Chế độ Đơn giản / Chuyên gia

mode-label = Chế độ
mode-simple = Đơn giản
mode-expert = Chuyên gia
mode-simple-hint = Chỉ những gì đa số cần: danh sách rõ ràng và hành động an toàn.
mode-expert-hint = Hiện tất cả: bộ lọc nâng cao, thuộc tính, đường dẫn không đọc được, xuất JSON/CSV và lệnh nfd tương đương.
cli-hint = Lệnh:
cli-hint-title = Dòng lệnh nfd làm được việc tương tự (dùng cho script và Task Scheduler).
action-copy-command = Sao chép lệnh

## Cũ & lớn

nav-oldlarge = Cũ & lớn
ol-title = Tệp cũ & lớn
ol-intro = Cách giải phóng nhanh nhất: tệp rất lớn, tệp lớn lâu không ai sửa, và bộ cài cũ còn trong Downloads. Chọn thứ bạn không cần nữa — sau khi kiểm tra an toàn, chúng được chuyển vào Thùng rác.
ol-only-folder = Chỉ thư mục đang mở trong Phân tích
ol-large-min = Lớn nghĩa là từ
ol-stale-age = Cũ nghĩa là không sửa trong
ol-stale-min = Tệp cũ từ
ol-installer-age = Bộ cài cũ hơn
ol-months = { $count } tháng
ol-years = { $count } năm
ol-days = { $count } ngày
ol-summary = { $files } tệp · tổng { $size }
ol-large-title = Tệp lớn
ol-large-hint = Từ { $size } trở lên
ol-stale-title = Lâu không sửa
ol-stale-hint = Không đổi trong { $age }
ol-installers-title = Bộ cài cũ
ol-installers-hint = .exe .msi .iso .zip… trong Downloads, cũ hơn { $days } ngày
ol-files = { $count } tệp
ol-selected = Đã chọn { $count } · { $size }
ol-select-all = Chọn tất cả
ol-stale-note = Windows không ghi lại chính xác lần mở tệp gần nhất, nên "cũ" nghĩa là không bị sửa đổi.
ol-more = Đang hiện { $shown } tệp lớn nhất trong { $count }.
ol-none = Không có gì — gọn gàng rồi.

## Dọn gọn (thư mục rỗng, shortcut hỏng)

nav-tidy = Dọn gọn
tidy-title = Dọn gọn
tidy-intro = Thư mục rỗng và shortcut trỏ tới nơi không còn tồn tại. Chúng gần như không tốn dung lượng nhưng làm rối Explorer và menu Start.
tidy-readonly = Hiện chỉ để xem: tính năng xóa sẽ có cùng engine an toàn (Phase 3) để mọi thay đổi đều hoàn tác được.
tidy-empty-title = Thư mục rỗng
tidy-empty-summary = { $count } cây thư mục rỗng ({ $total } thư mục tất cả). Thư mục không đọc được sẽ không bao giờ bị liệt kê.
tidy-empty-none = Không có thư mục rỗng.
tidy-empty-self = Không có gì
tidy-nested = { $count } thư mục rỗng
tidy-links-title = Shortcut hỏng
tidy-links-checking = Đang kiểm tra shortcut…
tidy-links-summary = Đã kiểm tra { $checked } shortcut; { $unchecked } shortcut trỏ tới ổ mạng hoặc vị trí đặc biệt nên không kiểm tra.
tidy-links-none = Không có shortcut hỏng.
tidy-target = Đích
tidy-status-ok = Hoạt động
tidy-status-missing = Mất tệp đích
tidy-status-drive_missing = Ổ đĩa không kết nối
tidy-status-unchecked = Chưa kiểm tra

## Báo cáo HTML

analyze-export-html = Báo cáo (HTML)
report-title = Báo cáo ổ đĩa
report-generated = Tạo lúc { $date } · NANOFreeDisk { $version }
report-treemap = Thứ gì chiếm dung lượng
report-children = Mục lớn nhất
report-changes = Thay đổi kể từ { $date }
report-drive-share = Tỷ lệ trên ổ đĩa
report-footer = Tạo bởi NANOFreeDisk { $version } — chỉ phân tích, không thay đổi hay xóa gì. Không thu thập dữ liệu.

## Có gì thay đổi? (lịch sử quét)

analyze-tab-changes = Có gì thay đổi?
changes-history-off = Lịch sử quét đang tắt (Cài đặt → Lịch sử quét) nên chưa có gì để so sánh.
changes-not-recorded = Lần quét này không được lưu vào lịch sử nên không so sánh được.
changes-first = Đây là lần quét đầu tiên được lưu của thư mục này. Quét lại sau vài ngày để xem có gì thay đổi.
changes-compare = So với lần quét lúc
changes-summary = { $days ->
        [0] Từ lần đó
       *[other] Trong { $days } ngày
    }: { $size } · { $files } tệp
changes-free = Dung lượng trống của ổ { $size }
changes-none = Không có thư mục hay tệp lớn nào thay đổi quá 10 MB.
changes-grew = Tăng lên
changes-shrank = Giảm đi
changes-files-up = Tệp lớn mới hoặc to ra
changes-files-down = Tệp lớn bị xóa hoặc nhỏ đi
changes-new = mới
changes-gone = đã mất
changes-footnote = Mỗi thay đổi được hiện ở thư mục cụ thể nhất giải thích nó. Thư mục và tệp nhỏ không được lưu trong lịch sử nên thay đổi rất nhỏ bị bỏ qua.
history-save-failed = Quét xong nhưng không lưu được vào lịch sử: { $error }

## Điểm sức khỏe

health-title = Điểm sức khỏe
health-good = Tốt
health-fair = Cần chú ý
health-poor = Nên xử lý sớm
health-all-good = Hiện không có gì cần sửa.
health-basis = Dựa trên dung lượng trống, tốc độ đầy của từng ổ (từ lịch sử quét) và lần quét gần nhất. Sức khỏe phần cứng (S.M.A.R.T.) sẽ có ở Phase 7.
health-low-space = { $drive } chỉ còn trống { $percent } ({ $size }).
health-filling-fast = Với tốc độ hiện tại, { $drive } sẽ đầy sau khoảng { $days } ngày.
health-old-large = Có thể giải phóng { $size } tệp lớn hoặc cũ trong { $folder }.
health-no-scan = Hãy phân tích một ổ đĩa để điểm tính cả thứ đang chiếm chỗ.
health-no-trend = Quét lại { $drive } vào ngày khác để biết ổ đầy nhanh cỡ nào.
health-fix-oldlarge = Giải phóng
health-fix-analyze = Xem thứ tăng lên
health-fix-home = Phân tích ngay

settings-history = Lịch sử quét
settings-history-keep = Lưu lịch sử các lần quét
settings-history-hint = Lưu kích thước thư mục và tên tệp lớn (không bao giờ lưu nội dung tệp) của 30 lần quét gần nhất mỗi thư mục, chỉ trên máy này. Dùng cho "Có gì thay đổi?" và điểm sức khỏe.
settings-history-count = Đã lưu { $count } lần quét
settings-history-clear = Xóa lịch sử
settings-history-clear-confirm = Xóa mọi lần quét đã lưu? "Có gì thay đổi?" sẽ bắt đầu lại từ lần quét tới.
settings-history-cleared = Đã xóa lịch sử quét.

## Bách khoa thư mục

encyclopedia-title = Đây là gì?
encyclopedia-inside = Nằm trong:
encyclopedia-note = Từ Bách khoa thư mục (rules/encyclopedia). Chỉ để tham khảo — NANOFreeDisk không bao giờ tự xóa gì dựa trên nó.
risk-safe = Xóa an toàn
risk-caution = Xóa cẩn thận
risk-advanced = Chỉ dành cho chuyên gia
risk-protected = Không được xóa

## Heatmap tuổi tệp, chủ sở hữu, liên kết hỏng

analyze-tab-age = Tuổi tệp
age-hint = Dung lượng theo thời điểm tệp được sửa lần cuối. Ô càng đậm càng nhiều dữ liệu; bấm vào thư mục để mở.
age-col-0 = < 1 tháng
age-col-1 = 1–6 tháng
age-col-2 = 6–12 tháng
age-col-3 = 1–2 năm
age-col-4 = 2–5 năm
age-col-5 = > 5 năm
age-col-6 = Không có ngày
age-direct = Tệp nằm ngay ở đây
age-other = Phần còn lại
age-total = Tổng
details-owner = Chủ sở hữu
details-owner-hint = Tài khoản sở hữu mục này (theo thiết lập bảo mật của Windows).
tidy-junctions-title = Junction và symbolic link hỏng
tidy-junctions-summary = Đã kiểm tra { $checked } liên kết. Liên kết mất đích sẽ báo lỗi khi mở.
tidy-junctions-none = Không có junction hay symbolic link hỏng.

## Gợi ý "Nếu bấm thì…" (G4)

hint-scan = Nếu bấm: thư mục chỉ được đọc (không thay đổi gì) và nếu bật lịch sử quét, bản tóm tắt dung lượng được lưu trên máy này.
hint-rescan = Nếu bấm: thư mục được đọc lại và kết quả này được thay thế; nếu bật lịch sử quét, một snapshot mới được lưu.
hint-cancel-scan = Nếu bấm: quá trình quét dừng và kết quả dở dang bị bỏ. Không có gì trên ổ đĩa thay đổi.
hint-export = Nếu bấm: một tệp mới được ghi vào chỗ bạn chọn; không thay đổi gì khác.
hint-theme = Nếu bấm: màu sắc đổi ngay; có thể đổi lại bất cứ lúc nào.
hint-theme-import = Nếu bấm: màu tùy chỉnh hiện tại bị thay bằng màu trong tệp. Hãy xuất ra trước nếu muốn giữ.
hint-theme-reset = Nếu bấm: các thay đổi màu tùy chỉnh bị bỏ và màu của preset quay lại.
hint-theme-preset = Nếu bấm: theme chuyển sang Tùy chỉnh dựa trên preset này; màu tùy chỉnh trước đó bị thay.
hint-mode = Nếu bấm: chỉ thay đổi những gì được hiển thị; không đụng tới cài đặt hay tệp nào.
hint-language = Nếu bấm: ứng dụng đổi ngôn ngữ ngay lập tức.
hint-font = Nếu bấm: cỡ chữ đổi ngay.
hint-history-toggle = Nếu tắt: các lần quét mới không được lưu nữa; các lần đã lưu vẫn còn cho tới khi bạn xóa.
hint-history-clear = Nếu bấm: mọi lần quét đã lưu bị xóa khỏi máy này (có hỏi trước). Tệp của bạn không bị đụng tới.

## Tích hợp Explorer (A7)

shell-menu-caption = Phân tích bằng NANOFreeDisk
settings-shell = Tích hợp Explorer
settings-shell-toggle = Hiện "Phân tích bằng NANOFreeDisk" khi bấm chuột phải vào thư mục hoặc ổ đĩa
settings-shell-hint = Chỉ thêm cho tài khoản của bạn (không cần quyền admin). Tắt tùy chọn này hoặc gỡ NANOFreeDisk sẽ xóa nó. Nếu ứng dụng đang mở, thư mục sẽ mở trong cửa sổ đó.
settings-shell-on = Đã thêm vào menu Explorer.
settings-shell-off = Đã gỡ khỏi menu Explorer.
hint-shell = Nếu bấm: một mục được thêm vào (hoặc gỡ khỏi) menu chuột phải của Explorer, chỉ cho tài khoản của bạn; không thay đổi gì khác.

## Errors

error-generic = Đã xảy ra lỗi: { $message }
error-no-session = Hãy quét một ổ đĩa hoặc thư mục trước.
