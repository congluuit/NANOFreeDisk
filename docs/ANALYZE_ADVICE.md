# Phân tích → Gợi ý: "Xóa được không, và nếu xóa thì sao?"

> Trạng thái: **xong** (2026-10-17). Crate `nfd-advice`, lệnh `nfd advise` / `nfd scan --advice`, bảng **Gợi ý** trong Phân tích, cột đánh giá trong cây thư mục, thẻ "Có xóa được không?" trong bảng chi tiết, treemap tô màu theo đánh giá, Home và Health Score dùng chung tổng. Mọi thao tác xóa vẫn đi `Planner → SafetyGate → Executor → UndoJournal` (`docs/PHASE3_CLEAN.md`); lịch sử hội thoại AI không bao giờ là ứng viên (PLAN §5.1).

## 1. Vì sao

Sau khi quét, Phân tích chỉ *liệt kê* dung lượng; treemap thì rối. Người dùng phải tự đi tìm trong Tệp rác, Cache lập trình, Cũ & lớn… để biết cái gì xóa được. Ví dụ thật: `D:\Working\SelfProject\NANOFreeDisk\.claude` 82 GB — thực ra là worktree của các phiên Claude Code chứa đầy `target/`; người dùng muốn xóa nhưng không biết hậu quả.

Phân tích giờ là **nơi duy nhất gợi ý dọn gì**: mỗi đường dẫn của lần quét có một đánh giá, lý do, câu "nếu xóa…", những gì *không* bị đụng tới, và hành động đúng cách.

## 2. Đánh giá (verdict)

| Verdict | Nghĩa | Hành động |
|---|---|---|
| 🟢 `safe` — Có thể xóa an toàn | tạo lại tự động / không còn cần | tích sẵn, một cú bấm |
| 🟡 `caution` — Thận trọng | xóa được nhưng mất gì đó (lịch sử, tải lại, build lại lâu) | phải tự tích + xác nhận |
| 🟠 `guide` — Chỉ hướng dẫn | chỉ công cụ của Windows / của tool thu nhỏ được an toàn | chỉ giải thích |
| 🔴 `keep` — Không được xóa | Windows, chương trình, thông tin đăng nhập, lịch sử hội thoại, 🔒 Giữ, đang dùng | không có nút xóa |
| ⚪ `unknown` — Chưa rõ | không có thông tin (kể cả ảnh/video cá nhân) | người dùng tự quyết (Thùng rác) |

Rule mức 🟠 Advanced → `caution` + chỉ chọn được ở Expert.

## 3. Engine (`crates/nfd-advice`)

**Một lượt từ trên xuống** trên arena của lần quét (pre-order: thư mục cha luôn được xét trước con). Mỗi entry có thể *bắt đầu một unit* — phần cây mà một nguồn tri thức nhận cả khối. Thứ tự ưu tiên tại một nút:

1. Lịch sử hội thoại theo tên (`agent_history_entry`: `.aider.chat.history.md`, `.specstory`, cặp `.claude/projects`…) → 🔴.
2. Bài Bách khoa thư mục loại "chỉ hướng dẫn" (WinSxS, pagefile, hiberfil, System Volume Information, Windows.old, SoftwareDistribution, $Recycle.Bin, Package Cache, AppData\Local\Packages, Docker wsl, Installer) → 🟠; `.git` → 🔴.
3. SafetyGate (`check_path(Manual)`) — chỉ gọi ở nơi một template được bảo vệ có thể chạm tới (tập template + mọi tổ tiên, cha của template có wildcard, con trực tiếp của gốc ổ): `Protected` → 🔴 (system / programs / protected / lịch sử hội thoại theo template), `SystemDir` → 🔴 Windows.
4. Rule pack: chỉ hỏi những rule có thư mục gốc chứa nút này (`Matcher::bases` + `rules_matching_among`); rule 🔒 → 🔴; rule khác phải qua `check_path(Rule{level})`; `keep_newest` (bản mới nhất → 🔴 `newest`), `min_age_days` (tệp mới ghi → tính vào "giữ lúc này"), có lệnh chính thức của package store → hành động `store`.
5. Build output C9 (marker `Cargo.toml` → `target`…; tên chung chung `dist/build/bin/obj` cần git nên bỏ qua, để tab Repo) → mức theo `artifacts::level_for` (Rust `target` > 5 GB = 🟡).
6. Worktree của agent (`<dự án>/.claude/worktrees/<tên>`) → 🟡 *chặn* `git-check` cho tới khi git xác nhận.
7. Bài Bách khoa khác không có rule xác nhận (vd. `node_modules` không có `package.json`) → 🟡, Thùng rác (hoặc lệnh của store).
8. Ảnh / video / nhạc (template Pictures/Videos/Music, tên `Photos`, `DCIM`, `Ảnh`…) → ⚪ cá nhân, không bao giờ gợi ý xóa.
9. Tệp: bộ cài cũ trong `Downloads` (> 30 ngày), tệp ≥ 100 MB không sửa > 1 năm — cùng bộ lọc với Cũ & lớn → 🟡, Thùng rác.

**Bên trong một unit** chỉ quyền chặt hơn mới "khoét" được một phần: trong unit dọn được (rule, build output…) chỉ lịch sử hội thoại, rule 🔒, vị trí được bảo vệ, `.git`; trong unit 🔴 Windows / Program Files chỉ rule mà SafetyGate cho phép (vd. `Windows\Temp`) và bài 🟠; trong worktree chưa kiểm tra thì build output là unit riêng (cũng bị chặn `git-check`).

**Lượt từ dưới lên** cộng byte của mọi tệp theo verdict cho từng thư mục (`ByVerdict`) và riêng phần "giải phóng được bằng hành động không bị chặn" (`reclaim`). Một thư mục như `.claude` đọc ra "X GB xóa an toàn, Y GB phải giữ".

**Làm giàu** (sau lượt tĩnh, trong nền):
- `apply_ai(report)`: báo cáo AI leftovers (`nfd_devtools::ai_leftovers::scan` với `roots` = các dự án có `.claude/worktrees`): worktree sạch + đã merge/push → 🟢 cả cây (gỡ bằng `git worktree remove`), bẩn / chưa merge / phiên đang chạy (`session-live`) → 🔴 cả cây, build output được phép của worktree → unit riêng. Worktree không có trong báo cáo → `unverified`.
- `apply_duplicates(copies)`: bản sao thừa của lần tìm trùng lặp gần nhất → 🟡, hành động "Xem tệp trùng lặp".

**Gợi ý** (`suggestions`): unit gom theo (verdict, lý do, nguồn, lý do chặn, hành động) — Windows / Program Files / ảnh cá nhân / lịch sử hội thoại tách theo vị trí, worktree theo thư mục `worktrees`; 20 mục lớn nhất mỗi nhóm, vị trí chung; nhóm 🟢/🟡 giữ hết, nhóm 🟠/🔴/⚪ tối đa 12 và ≥ 1 MiB. `elsewhere(report)`: rác các rule tìm thấy *ngoài* thư mục đã quét (lần quét Tệp rác gần nhất).

**Hành động** (`prepare`): rule → chạy lại chính rule đó (`Matcher::scan` chỉ với các id đó) và chỉ lấy ứng viên bên trong thư mục đã chọn; build output → quarantine như mục tự chọn; AI → `ai_leftovers::prepare` (quarantine + git); cá nhân → Thùng rác. 🟡 cần `caution`, 🟠 cần `expert`; unit 🔴/🟠/bị chặn bị từ chối; thư mục chứa unit phải giữ không bao giờ đi nguyên khối (`contains-kept`).

Văn bản "nếu xóa…" / "không bị đụng tới" EN + VI nằm trong dữ liệu (rule pack, catalog AI, bách khoa) và `text.rs` cho kết luận của engine; nhãn giao diện ở `locales/{en,vi}/advice.ftl`.

## 4. Ứng dụng

- Tauri `commands/advice.rs`: tính lại sau mỗi lần quét, mỗi lần xóa trong Phân tích và sau `advice_clean` (sự kiện `advice-ready`, lần hai với `enriched`); `advice_summary`, `advice_detail`, `advice_clean(keys, caution, expert)` (khóa nhóm, `entry|<id>`, `within|<id>`, `elsewhere|<rule>`), `advice_cancel`, `advice_refresh(rulesDir)` (chỉ cho gui-smoke). Activity kind `advice` (quarantine) + `worktree` (git) + `manual` (Thùng rác). `children` / `detail` có `advice`, `slice` có `verdicts`.
- Phân tích: tab **Gợi ý** mặc định bên phải (Treemap / Sunburst thành tab, treemap có "Màu theo đánh giá" và ít nhãn hơn); cây có cột Đánh giá + Giải phóng được; bảng chi tiết có thẻ "Có xóa được không?" (lý do, nếu xóa, không bị đụng tới, bên trong theo verdict, các phần lớn nhất, nút đúng: dọn theo quy tắc / quarantine / git / lệnh chính thức / Thùng rác; mục 🔴/🟠 không có nút xóa, thanh chọn của cây không cho tích).
- Home "Giải phóng X GB — xem gợi ý" và Health Score (`reclaimable` thay `old-large`; `junk` chỉ tính rác ngoài thư mục đã quét) dùng cùng tổng.
- Simple: nhóm 🟠 hiện nhưng khóa ("Chỉ ở chế độ Chuyên gia"), ẩn id rule; Expert: hiện nguồn, lệnh `nfd advise`.

## 5. CLI

`nfd advise <path> [--json] [--lang vi] [--top N] [--no-git] [--rules DIR]`, `nfd scan <path> --advice`. JSON: `nanofreedisk.advice.v1` (`docs/schema/advice-v1.schema.json`). Chỉ đọc.

## 6. Kiểm thử & đo

- `cargo test -p nfd-advice`: fixture có `Windows` (System32 🔴, WinSxS 🟠, `Windows\Temp` theo rule 🟢), Program Files, `~\.claude\projects` + `.aider.chat.history.md` + `.specstory` trong thư mục rule (luôn 🔴, không bao giờ vào plan), rule 🔒 bên trong rule dọn được, `min_age_days`, package store, `target` có/không marker, worktree `.claude/worktrees/wt1` trước git (bị chặn, `reclaim` = 0), sau git sạch (🟢 cả cây, chỉ `ai_paths`) và phiên đang chạy (🔴 cả cây), ảnh cá nhân, bộ cài cũ (🟡 cần xác nhận), trùng lặp, tổng cộng khớp `on_disk` từng thư mục, JSON ổn định; `nfd-core` `advice_replaces_old_and_large`.
- UI: `advice.test.ts` (chia mục, tích sẵn, khóa, cần xác nhận, khóa dịch EN/VI, token màu).
- `gui-smoke`: fixture riêng (quét không ghi lịch sử, pack rule riêng): nhóm và verdict, cột cây, thẻ chi tiết, treemap, tab Gợi ý + hộp xác nhận, nhóm 🔴 và 🟡 chưa xác nhận bị từ chối, dọn vào quarantine + khôi phục, Health không còn `old-large`.
- Máy tham chiếu (chỉ đọc, bản debug): `nfd advise D:\Working\SelfProject\NANOFreeDisk` ~6,5 s (gồm quét + git) — 8 worktree đều 🔴 `session-live` (phiên còn chạy), `target` 36 GB 🟡; `nfd advise %LOCALAPPDATA%` (863 nghìn tệp) ~10 s trong đó quét ~4,6 s.
