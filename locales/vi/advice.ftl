# Phân tích → Gợi ý: cái gì xóa được, cái gì cần thận trọng, cái gì phải giữ, và vì
# sao (nfd-advice). Nạp cùng app.ftl; mọi key phải có trong en/advice.ftl.

## Đánh giá
advice-verdict-safe = Có thể xóa an toàn
advice-verdict-caution = Thận trọng
advice-verdict-guide = Chỉ hướng dẫn
advice-verdict-keep = Nên giữ
advice-verdict-unknown = Chưa rõ
advice-badge-safe = An toàn
advice-badge-caution = Thận trọng
advice-badge-guide = Hướng dẫn
advice-badge-keep = Nên giữ
advice-badge-unknown = —
advice-badge-partly = Gỡ được { $size }
advice-verdict-hint-safe = Được tạo lại tự động hoặc không còn cần nữa.
advice-verdict-hint-caution = Có thể xóa, nhưng sẽ mất một thứ gì đó (lịch sử, phải tải lại, phải build lại lâu).
advice-verdict-hint-guide = Chỉ lệnh của chính Windows hoặc của công cụ mới thu nhỏ được an toàn.
advice-verdict-hint-keep = Ứng dụng khuyên giữ lại (đang dùng, chưa lưu ở nơi khác, thiết lập…). Ứng dụng có thể sai: bạn vẫn có thể Vẫn xóa, khôi phục được từ khu cách ly.
advice-verdict-hint-unknown = Không có thông tin gì: chỉ bạn quyết định được.
advice-verdict-protected = Được bảo vệ
advice-badge-protected = Bảo vệ
advice-verdict-hint-protected = Bảo vệ cứng: Windows, chương trình, thông tin đăng nhập hoặc lịch sử hội thoại AI. Không bao giờ bị xóa, kể cả khi bạn yêu cầu.

## Bảng gợi ý
advice-tab = Gợi ý
advice-headline = Có thể giải phóng { $size }
advice-headline-none = Hiện không có gì ở đây có thể giải phóng an toàn.
advice-headline-sub = Trong { $total } đã quét: mỗi mục bên dưới cho biết điều gì xảy ra nếu xóa, và cái gì được giữ lại.
advice-computing = Đang xem xét cái gì có thể xóa và cái gì phải giữ…
advice-checking-git = Đang kiểm tra worktree của AI agent bằng git…
advice-not-ready = Gợi ý sẽ hiện ở đây ngay khi xem xét xong.
advice-section-safe = Xóa an toàn
advice-section-safe-hint = Được tạo lại tự động hoặc không còn cần: đã được tích sẵn.
advice-section-caution = Xem lại trước
advice-section-caution-hint = Sẽ mất một thứ gì đó (lịch sử, phải tải lại, phải build lại lâu): chỉ tích những gì bạn chắc chắn.
advice-section-guide = Chỉ hướng dẫn
advice-section-guide-hint = Lệnh của chính Windows hoặc công cụ mới thu nhỏ được các mục này; NANOFreeDisk chỉ cách làm.
advice-section-keep = Lớn nhưng nên giữ
advice-section-keep-hint = Lý do từng mục được giữ. Những gì chỉ do ứng dụng đánh giá vẫn có thể Vẫn xóa; các mục bảo vệ cứng (Windows, chương trình, thông tin đăng nhập, lịch sử hội thoại) thì không bao giờ.
advice-section-elsewhere = Rác ở nơi khác trên máy
advice-section-elsewhere-hint = Do các quy tắc dọn dẹp tìm thấy bên ngoài thư mục đã quét.
advice-empty-section = Không có gì.
advice-expert-only = Chỉ ở chế độ Chuyên gia
advice-col-verdict = Đánh giá
advice-col-reclaim = Giải phóng được
advice-color-type = Màu theo loại tệp
advice-color-verdict = Màu theo đánh giá

## Lý do (tiêu đề)
advice-reason-rule = { $app }
advice-reason-keep-rule = { $app } (được giữ)
advice-reason-build-output = Kết quả build { $kind }
advice-reason-agent-worktree = Worktree của AI agent
advice-reason-agent-leftover = Phần thừa của { $app }
advice-reason-agent-history = Lịch sử hội thoại { $app }
advice-reason-system = Windows
advice-reason-programs = Chương trình đã cài
advice-reason-protected = Vị trí được bảo vệ
advice-reason-personal = Ảnh, video và nhạc của bạn
advice-reason-explained = { $title }
advice-reason-old-large = Tệp lớn và cũ
advice-reason-old-installer = Bộ cài cũ trong Downloads
advice-reason-duplicate = Bản sao trùng lặp
advice-in-folders = { $count ->
   *[other] trong { $count } thư mục
}
advice-items-more = và { $count } mục khác
advice-recent = { $size } mới ghi gần đây được giữ lại lúc này
advice-if-delete = Nếu xóa
advice-why-keep = Vì sao giữ lại
advice-untouched = Những gì không bị đụng tới
advice-close-first = Đóng trước: { $apps }
advice-store-better = Lệnh của chính trình quản lý gói làm việc này: { $id }

## Vẫn xóa (người dùng quyết định thay ứng dụng)
advice-override = Vẫn xóa…
advice-override-hint = Ứng dụng khuyên giữ lại nhưng có thể sai. Bạn quyết định: mục sẽ vào khu cách ly (khôi phục được trong 7 ngày).
advice-override-title = Vẫn xóa?
advice-override-body = { $count ->
   *[other] NANOFreeDisk khuyên giữ lại { $count } mục này ({ $size }). Có thể bạn biết rõ hơn.
}
advice-override-why = Vì sao ứng dụng giữ lại
advice-override-ways = Không có gì bị xóa vĩnh viễn: worktree git không còn thay đổi chưa commit được gỡ bằng `git worktree remove` (nhánh và commit vẫn còn); mọi thứ khác vào khu cách ly và khôi phục được trong Lịch sử dọn dẹp suốt 7 ngày.
advice-override-ack = Tôi hiểu và vẫn muốn xóa
advice-override-go = Vẫn xóa
advice-override-done = Đã dời { $size } đi ({ $count } mục). Có thể khôi phục trong Lịch sử dọn dẹp nếu cần.
advice-override-skipped = { $count } mục được bảo vệ cứng nên không bị đụng tới.
advice-hard-hard = Bảo vệ cứng: Windows, chương trình, thông tin đăng nhập hoặc lịch sử hội thoại AI không bao giờ bị xóa, kể cả khi bạn yêu cầu.
advice-hard-inside-hard = Bảo vệ cứng: mục này nằm trong Windows, thư mục chương trình, thông tin đăng nhập hoặc lịch sử hội thoại.
advice-hard-contains-hard = Bên trong có thứ được bảo vệ (lịch sử hội thoại, thông tin đăng nhập…) nên không thể xóa nguyên khối: hãy xóa các phần khác bên trong.
advice-hard-root = Không xóa chính thư mục đã quét từ đây.
advice-hard-other = SafetyGate từ chối ({ $code }).

## Vì sao chưa thể
advice-blocked-git-check = Đang chờ git kiểm tra…
advice-blocked-unverified = git chưa xác nhận: hãy kiểm tra trong Cache lập trình → Rác của AI.
advice-blocked-session-live = Phiên AI của nó vẫn đang làm việc (hoạt động trong 24 giờ qua).
advice-blocked-session-open = Phiên của nó vẫn đang mở trong ứng dụng Claude: hãy lưu trữ (archive) phiên đó trước.
advice-blocked-dirty = Còn thay đổi chưa commit.
advice-blocked-not-merged = Nhánh của nó chưa được merge cũng chưa được push.
advice-blocked-not-removable = git chưa gỡ được (còn thay đổi chưa commit hoặc nhánh chưa merge).
advice-blocked-newest = Bản mới nhất được giữ lại.
advice-blocked-contains-history = Bên trong có lịch sử hội thoại.
advice-blocked-tracked = git đang theo dõi tệp bên trong.
advice-blocked-other = Hiện chưa thể ({ $code }).

## Hành động
advice-action-clean = Dọn theo quy tắc
advice-action-quarantine = Chuyển vào khu cách ly
advice-action-ai-leftovers = Gỡ bằng git
advice-action-recycle-bin = Chuyển vào Thùng rác
advice-action-store = Chạy lệnh chính thức
advice-action-duplicates = Xem tệp trùng lặp
advice-action-guide = Cách thu nhỏ
advice-action-show = Xem trong cây thư mục
advice-action-hint-clean = Nếu bấm: chính quy tắc đó quét lại và chuyển những gì nó bao phủ vào khu cách ly (7 ngày để khôi phục); tệp mới ghi và đường dẫn 🔒 Giữ lại không bị đụng tới.
advice-action-hint-quarantine = Nếu bấm: thư mục được chuyển vào khu cách ly và có thể khôi phục trong Hoạt động suốt 7 ngày.
advice-action-hint-ai-leftovers = Nếu bấm: kết quả build vào khu cách ly, sau đó `git worktree remove` (không bao giờ --force) gỡ worktree; nhánh được giữ nguyên.
advice-action-hint-recycle-bin = Nếu bấm: mục được chuyển vào Thùng rác, có thể khôi phục từ đó.
advice-action-hint-store = Mở Cache lập trình → Package store, nơi lệnh chính xác được hiển thị trước khi chạy.
advice-action-hint-duplicates = Mở Tệp trùng lặp để chọn bản sao được giữ lại.
advice-action-hint-guide = Hiển thị giải thích về mục này và công cụ phù hợp.
advice-clean-n = Dọn { $count } mục đã chọn ({ $size })
advice-clean-hint = Nếu bấm: quy tắc và kết quả build vào khu cách ly (7 ngày để khôi phục), worktree của phiên AI đã xong được gỡ bằng git, tệp cá nhân bạn chọn vào Thùng rác.

## Xác nhận
advice-confirm-title = Dọn các gợi ý đã chọn?
advice-confirm-body = { $count ->
   *[other] { $count } gợi ý, khoảng { $size }.
}
advice-confirm-ways = Quy tắc và kết quả build vào khu cách ly (khôi phục được trong 7 ngày), worktree của phiên AI đã xong được gỡ bằng git (nhánh được giữ), tệp cá nhân bạn chọn vào Thùng rác.
advice-confirm-note = Mọi mục được SafetyGate kiểm tra lại ngay trước khi đụng tới. Lịch sử hội thoại, vị trí được bảo vệ và đường dẫn 🔒 Giữ lại không bao giờ nằm trong đó.
advice-confirm-go = Dọn
advice-done = Đã giải phóng { $size } ({ $count } mục).
advice-done-problems = { $count } mục bị bỏ qua hoặc lỗi: xem trong Hoạt động.
advice-skipped = { $count } gợi ý không được dọn (chưa xác nhận, chỉ ở chế độ Chuyên gia hoặc chứa thứ được giữ).

## Bảng chi tiết
advice-details-title = Có xóa được không?
advice-details-loading = Đang kiểm tra…
advice-details-inside = Bên trong
advice-details-parts = Gồm những gì
advice-details-part-of = Thuộc { $path }
advice-details-unknown = Không có thông tin gì: chỉ bạn quyết định được. Nếu tự xóa, mục sẽ vào Thùng rác.
advice-details-mixed = Hỗn hợp: { $free } có thể giải phóng qua gợi ý, phần còn lại được giữ.
advice-free-here = Giải phóng { $size } ở đây
advice-details-nothing = Không có gì bên trong mà ứng dụng có thể giải phóng.
advice-details-open-suggestion = Xem trong Gợi ý

## Trang chủ, Điểm sức khỏe, Hoạt động
home-free-advice = Giải phóng { $size } — xem gợi ý
home-free-advice-sub = Từ lần quét gần nhất: cái gì xóa được, cái gì cần thận trọng và cái gì phải giữ.
health-reclaimable = Có thể giải phóng { $size } trong { $folder } (xem gợi ý).
health-fix-suggestions = Xem gợi ý
activity-kind-advice = Gợi ý (Phân tích)
activity-kind-override = Vẫn xóa (bạn quyết định)
