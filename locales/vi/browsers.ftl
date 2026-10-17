# Dọn dẹp → "Bộ đệm trình duyệt web" → "Nâng cao: dọn một trang web"
# (BrowserSites.svelte, crates/nfd-browsers). Mọi khóa phải có trong en/browsers.ftl.

br-title = Nâng cao: dọn một trang web
br-hint = Chỉ xóa bộ đệm của các trang web được chọn, trong từng hồ sơ trình duyệt
br-intro = Tick các trang web để chỉ xóa bộ đệm của riêng chúng: script đã biên dịch (Code Cache), bộ đệm Service Worker và, với Firefox, bộ đệm web. Cookie, thông tin đăng nhập, Local Storage, IndexedDB, lịch sử, mật khẩu và tiện ích mở rộng không bao giờ bị đọc hay động đến. Hãy đóng trình duyệt trước; mọi thứ được chuyển vào khu cách ly và khôi phục được trong Hoạt động suốt 7 ngày.
br-blockfile-note = Bộ đệm web (HTTP) của Chromium trên Windows dùng định dạng "blockfile": mọi trang dùng chung một tệp chỉ mục và các tệp dữ liệu, nên xóa riêng một trang ở đó nghĩa là phải sửa trực tiếp các tệp này và có thể làm hỏng toàn bộ bộ đệm. Phần đó chỉ được hiển thị để tham khảo và được giữ nguyên; nó chỉ được dọn cả khối, bằng quy tắc ở trên.
br-loading = Đang đọc bộ đệm trình duyệt… (chỉ đọc)
br-none = Không tìm thấy bộ đệm trình duyệt nào có trang web.
br-search = Tìm trang web
br-rescan = Đọc lại bộ đệm
br-profile = hồ sơ { $name }
br-sites = { $count } trang web
br-running = { $exe } đang chạy — hãy đóng trước; không có gì được dọn trong trình duyệt này khi nó đang chạy.
br-site-listed = + { $size } trong bộ đệm web (giữ nguyên)
br-listed-only-hint = Chỉ nằm trong bộ đệm web blockfile: được liệt kê, không dọn theo từng trang.
br-no-match = Không có trang web nào khớp với từ khóa.
br-store-http-cache = Bộ đệm web
br-store-code-cache-js = Bộ đệm JavaScript
br-store-code-cache-wasm = Bộ đệm WebAssembly
br-store-cache-storage = Bộ đệm Service Worker
br-store-firefox-cache = Bộ đệm Firefox
br-more = Xem thêm { $count }
br-clean-n = Dọn { $count } trang web ({ $size })
br-clean-hint = Nếu bấm: bạn xem chính xác những gì sẽ bị xóa trước; các mục được chuyển vào khu cách ly.
br-confirm-title = Dọn bộ đệm của các trang web này?
br-confirm-body = { $entries } mục bộ đệm ({ $size }) của { $count } trang web sẽ được chuyển vào khu cách ly:
br-confirm-store = { $entries } · { $size }
br-confirm-index = { $count } tệp chỉ mục bộ đệm cũng được chuyển đi: trình duyệt sẽ tạo lại ở lần mở sau.
br-confirm-listed = { $size } của các trang này vẫn nằm trong bộ đệm web blockfile (không sửa theo từng trang).
br-confirm-running = Hãy đóng { $apps } trước: không có gì được dọn trong đó khi nó đang chạy.
br-confirm-nothing = Hiện không có gì để dọn cho các trang web này.
br-confirm-note = Chỉ chuyển nguyên các mục bộ đệm; cookie, thông tin đăng nhập và dữ liệu trang được giữ nguyên. Mọi thứ khôi phục được trong Hoạt động suốt 7 ngày.
br-done = Đã dọn bộ đệm trang web: { $size } · { $done } mục.
activity-kind-browser-site = Đã dọn bộ đệm trang web
reason-app-running = Ứng dụng đã được mở lại trong lúc dọn; bỏ qua.
