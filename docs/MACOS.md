# NANOFreeDisk trên macOS

> Ghi chú nội bộ (tiếng Việt). Phần hướng dẫn cho người dùng (tiếng Anh) nằm trong
> README, mục **macOS**.

Trạng thái: **bản xem trước** — build được cho Intel (x86_64) và Apple silicon (arm64)
trong một app *universal*, CI kiểm tra clippy + test trên `macos-latest`. **Chưa được
thử trên máy Mac thật** (người viết port này không có máy Mac): danh sách kiểm thử ở
§7 là việc cần làm trước khi coi bản macOS là ổn định.

## 1. Phạm vi

Mục tiêu đầu tiên: dọn máy Mac Intel của tác giả, chủ yếu là **tệp trùng**. Hai tính
năng bắt buộc chạy đúng:

- **Analyze** — cái gì chiếm ổ đĩa (cây thư mục, treemap/sunburst, danh sách, gợi ý).
- **Duplicates** — tìm và xóa tệp trùng.

Chạy được trên macOS vì phần lõi không phụ thuộc Windows: Old & Large, Tidy (thư mục
rỗng), DevCache, Activity (hoàn tác), Settings, bảng lệnh ⌘K, EN/VI, theme.

Ẩn trên macOS (chỉ có ở Windows, xem `apps/nanofreedisk/src/lib/platform.ts`):

| Tính năng | Lý do |
|---|---|
| Clean (gói luật hệ thống) | Các rule pack hiện là của Windows (`platform = ["windows"]`). |
| Drive health | SMART/NVMe qua IOCTL và helper nâng quyền của Windows. |
| Hardware (cảm biến, Tasks, PawnIO, nhiệt độ 24 h) | WMI / PawnIO / Toolhelp. |
| Monitor (hoạt động đĩa) | ETW + bộ đếm I/O của Windows. |
| Programs (gỡ cài đặt, phần sót lại, rác phần mềm) | Registry Uninstall, trình gỡ của Windows, gói Store (`nfd-apps`; test của crate chỉ chạy trên Windows). |
| Menu Explorer, cập nhật tại chỗ | Registry / NSIS–MSI; trên macOS Settings → Updates chỉ mở trang Releases. |

Các lệnh Rust của những tính năng trên vẫn được đăng ký (để `generate_handler!` không
đổi) nhưng trả lỗi `unsupported` ngoài Windows; giao diện không gọi tới chúng
(`hasFeature`), và các vòng đồng bộ nền (nhiệt độ, monitor, health) không chạy.

## 2. Thay đổi kiến trúc

Mọi thao tác xóa vẫn đi đúng một đường: **Planner → SafetyGate → Executor →
UndoJournal**. Không có đường xóa riêng cho macOS.

### 2.1 `nfd-platform`

- **Thùng rác** (`trash.rs`): `NSFileManager trashItemAtURL:resultingItemURL:error:`
  (crate `objc2-foundation`, cùng phiên bản Tauri đã dùng) — giống "Move to Trash" của
  Finder, "Put Back" vẫn dùng được. Đường dẫn trong Thùng rác được lưu vào
  `ItemResult.stored`, nên Activity → Restore đưa tệp về chỗ cũ
  (`trash::restore_trashed`). Thùng rác của macOS không cho app khác đọc nơi gốc, nên
  phải tự ghi lại. `journal::op_dirs` chỉ xét các mục quarantine/hardlink/clone, nên
  đường dẫn trong Thùng rác **không bao giờ** bị purge.
- **Ổ đĩa** (`volumes.rs`): `getfsstat` — `/` (ổ khởi động, dung lượng theo container
  APFS) và các ổ Finder hiển thị trong `/Volumes`; bỏ các volume ẩn của nhóm hệ thống
  (`/System/Volumes/*`).
- **Metadata** (`fsinfo.rs`): `st_flags` → `SF_DATALESS` (tệp iCloud/File Provider chưa
  tải về) thành `OFFLINE`, `UF_HIDDEN`, `UF_COMPRESSED`, `SF_RESTRICTED` (SIP) thành
  `SYSTEM`, cờ khóa thành `READONLY`.
- **Nhóm volume APFS**: từ macOS 10.15 `/` là volume hệ thống chỉ đọc, dữ liệu người
  dùng nằm ở volume *Data* gắn vào qua firmlink (`/Users`, `/Applications`, …). Volume
  Data báo cùng `volume_id` với `/` để máy quét đi vào `/Users` khi quét `/`; còn đường
  thứ hai tới nó, `/System/Volumes/Data`, là `MountPoint` (không quét hai lần). Vì hai
  volume có thể trùng số inode, `file_id` = `(st_dev << 64) | st_ino`.
- **Đọc nội dung** (`io.rs`): luồng băm ở QoS *utility*, `IOPOL_THROTTLE`, và
  `IOPOL_MATERIALIZE_DATALESS_FILES_OFF` — luồng đó không thể kích hoạt tải tệp từ
  đám mây (đọc sẽ lỗi thay vì tải). `open_sequential` còn từ chối tệp `SF_DATALESS`.
- **APFS clone** (`link.rs`): `clonefile(2)` cho "thay bản trùng bằng bản clone"
  (Finder Duplicate / `cp -c` làm y như vậy) và `clone_info` (`getattrlist`
  `ATTR_CMNEXT_CLONEID` + `EF_MAY_SHARE_BLOCKS`) để biết hai tệp đã dùng chung block.
  HFS+ dùng hard link; FAT/exFAT/mạng thì không.
- **Full Disk Access** (`privacy.rs`): thử đọc TCC.db / `~/Library/Safari|Mail|Messages`
  → `Some(true|false)`; `app_info.fullDiskAccess` mang kết quả lên giao diện.
- **Thư mục chuẩn**: `APPDATA` = `~/Library/Application Support` (Claude Desktop,
  VS Code, Cursor, Windsurf, Antigravity… lưu ở đây như `%APPDATA%` trên Windows, nên
  các template lịch sử AI hiện có áp dụng luôn), `LOCALAPPDATA` = `~/Library/Caches`.
  Ba biến chỉ có trên macOS: `MACOS_ROOT` (`/`), `MACOS_HOME`, `MACOS_TEMP_REAL`
  (thư mục tạm với `/var` → `/private/var`).

### 2.2 `nfd-core::safety` (`safety/macos.rs`)

Template dùng `${MACOS_ROOT}` / `${MACOS_HOME}` nên **không bao giờ** khớp trên Windows;
cổng chỉ bật luật gói (package) khi `MACOS_ROOT` được giải. Chi tiết ở §3.

### 2.3 `nfd-dupes`

- Các bản là **clone APFS thuần** của nhau (cùng clone id) chỉ tính một lần trong
  `wasted`; bản nào dùng chung block được gắn cờ `clone` ("bản sao APFS") và **không
  bao giờ được chọn sẵn** (`keep::sticky`). Nhóm chỉ gồm các clone của nhau bị bỏ
  (`SkipReason::Clone`) — xóa một bản không giải phóng gì.
- Hard link vẫn như cũ: nhiều tên của *một* tệp (cùng volume + file id) là một bản, không
  phải trùng lặp.
- Tệp dataless bị bỏ ở bước collect (`OFFLINE` → `cloud`) và bị kiểm tra lại ngay trước
  khi mở (`Source::info`), cộng thêm hai lớp ở `io.rs`.
- Bên trong gói (`.app`, `.photoslibrary`, tài liệu iWork…) bị SafetyGate chặn nên không
  bao giờ được băm hay đề xuất. `Mobile Documents` và `CloudStorage` là thư mục đám mây.
- `~/Library` và `~/Applications` là *thư mục ứng dụng* (như AppData / Program Files):
  dữ liệu app tự quản lý, chỉ được xét khi bật "include app folders" và không bao giờ
  chọn sẵn.

### 2.4 Ứng dụng (Tauri)

- `tauri.macos.conf.json` (Tauri tự gộp khi build trên macOS): gói `app` + `dmg`,
  `minimumSystemVersion: "11.0"`, `signingIdentity: "-"` (ký ad-hoc).
- `Info.plist` (Tauri tự gộp): câu giải thích cho các hộp thoại quyền Desktop /
  Documents / Downloads / ổ rời / ổ mạng.
- `vite.config.ts`: build cho `safari16` khi `TAURI_ENV_PLATFORM=darwin` (Windows vẫn
  `chrome110`, không đổi).
- `MacAccessHint.svelte`: khi chưa có Full Disk Access, một thẻ nổi trong các màn hình
  quét chỉ cách cấp quyền, có nút mở thẳng System Settings.

## 3. Những gì không bao giờ bị xóa trên macOS

Với **mọi nguồn** (chọn tay, Duplicates, luật, DevCache):

- Thư mục hệ thống ở gốc ổ khởi động: `/System`, `/Library`, `/Applications`, `/usr`,
  `/bin`, `/sbin`, `/opt`, `/private`, `/etc`, `/var`, `/tmp`, `/cores`, `/dev`,
  `/Network`, `/home`, `/net`, `/.vol`, `/.file`. Ngoại lệ duy nhất: **bên trong** thư
  mục tạm của người dùng (`$TMPDIR`, cả dạng `/var/folders/…` lẫn
  `/private/var/folders/…`), như `%TEMP%` trên Windows; bản thân thư mục đó vẫn là
  container.
- `/Volumes` (container) và gốc mỗi ổ trong đó; ở gốc mọi ổ: `.Spotlight-V100`,
  `.fseventsd`, `.Trashes`, `.DocumentRevisions-V100`, `.TemporaryItems`,
  `.TimeMachine`, `.MobileBackups`, `Backups.backupdb` (Time Machine), cùng các tên
  Windows đã có (`Windows`, `$Recycle.Bin`… — ổ Boot Camp).
- Của người dùng: iCloud Drive (`~/Library/Mobile Documents`, `CloudDocs`,
  `FileProvider`), Mail, Messages, Keychains, Accounts, Calendars, Safari, Cookies,
  HomeKit, IdentityServices, Preferences, `~/Library/Photos`, danh bạ, lịch sử cuộc
  gọi, TCC, bản sao lưu iPhone/iPad (`MobileSync`), `Containers`, `Group Containers`,
  `Daemon Containers`, Thùng rác (`~/.Trash`), thư viện Music/iTunes/TV/Photo Booth.
- **Gói** (`PACKAGE_EXTENSIONS`: `.app`, `.framework`, `.bundle`, `.plugin`, `.kext`,
  `.pkg`, `.pages`, `.numbers`, `.key`, `.rtfd`, `.xcodeproj`, `.logicx`…): không bao
  giờ xóa hay dedupe thứ gì *bên trong*; cả gói thì được chọn tay (vd. một app cũ trong
  Downloads) — trừ khi nằm trong vùng được bảo vệ (`/Applications`).
- **Thư viện** (`LIBRARY_EXTENSIONS`: `.photoslibrary`, `.musiclibrary`, `.tvlibrary`,
  `.imovielibrary`, `.fcpbundle`, `.lrlibrary`, `.sparsebundle`, `.backupbundle`…):
  không xóa cả khối lẫn bên trong — hãy dùng app sở hữu nó.
- **Lịch sử hội thoại AI** (PLAN §5.1, `AGENT_HISTORY_TEMPLATES`): các template sẵn có
  (`~/.claude`, `~/.codex`, `~/.gemini`, `~/.cursor/chats`, `${APPDATA}/Claude/...`,
  `${APPDATA}/Code|Cursor|Windsurf|Antigravity|Trae|Kiro|Devin/User/...`…) áp dụng
  nguyên vẹn vì `APPDATA` = `~/Library/Application Support`; thêm riêng macOS: ChatGPT
  (`com.openai.chat`), Codex, Copilot for Xcode, Ollama, Zed (`threads`,
  `~/.config/zed/conversations`), Raycast.

Không bị luật đụng tới, nhưng chọn tay từng mục bên trong được: `~/Movies`, `~/Public`,
`~/Library/CloudStorage` (Dropbox/OneDrive/Google Drive — xóa ở đây là xóa trên mây),
cùng Documents/Desktop/Pictures/Music/Downloads như trên Windows. Không xóa được bản
thân (container): `~`, `~/Library`, `~/Library/Application Support`, `~/Library/Caches`,
`~/Library/Developer`, `~/Applications`, `/Users`.

## 4. Tệp tin trên APFS — những điểm dễ sai

| Trường hợp | Cách xử lý |
|---|---|
| Clone APFS (Finder Duplicate, `cp -c`) | Cùng clone id → tính một lần; gắn cờ, không chọn sẵn; nhóm toàn clone bị bỏ (`clone`). |
| Tệp từng là clone rồi bị sửa (`EF_MAY_SHARE_BLOCKS`) | Gắn cờ `clone` (có thể giải phóng ít hơn kích thước). |
| Hard link | Nhiều tên của một tệp, không phải trùng lặp (như trước). |
| Tệp iCloud chưa tải (`SF_DATALESS`) | Không bao giờ mở/đọc/băm; đếm vào `cloud`. Với tay: cảnh báo "cloud placeholder". |
| Bên trong `.app` / thư viện | Bị cổng chặn, không băm. |
| `/System/Volumes/Data` | Ranh giới mount, không quét hai lần. |
| Ổ ngoài (exFAT/FAT) | Không có hard link/clone; xóa → `.Trashes` của ổ đó. |

## 5. Build, CI và phát hành

- **Script**: `scripts/build-macos.sh <out> <version>` — `pnpm tauri build --target
  universal-apple-darwin`, kiểm tra `lipo` (có cả `x86_64` và `arm64`), `codesign
  --verify --deep --strict`, `LSMinimumSystemVersion = 11.0`, rồi xuất
  `NANOFreeDisk-<ver>-macos-universal.dmg` và `…app.zip` (nén bằng `ditto` để giữ chữ
  ký).
- **CI** (`.github/workflows/ci.yml`): job `macos` (clippy `-D warnings`, nextest,
  doctest, dry run của `nfd clean` không đề xuất gì protected) chạy trên mọi push/PR
  và nằm trong cổng `CI OK`. Job `macos-app` chỉ chạy khi **Run workflow** bằng tay
  (Actions → CI → Run workflow) và để lại artifact `NANOFreeDisk-macos-universal`.
- **Release** (`release.yml`): job `build-macos` (sau `release-check`, vẫn chạy trên
  Windows và vẫn chặn) đưa `.dmg` và `.app.zip` vào release; `release-notes.sh` thêm dòng
  tải cho macOS. `latest.json` chưa có mục `darwin-*`: app macOS không tự cập nhật.
- **Kiểm tra trên máy Windows** (không có Mac): `scripts/…` không chạy được, nhưng có
  thể type-check: `rustup target add x86_64-apple-darwin` rồi `cargo clippy --target
  x86_64-apple-darwin` với một `cc`/`ar` giả (các build script C chỉ cần tạo ra tệp).

### 5.1 Vì sao `minimumSystemVersion = 11.0`

- Giao diện chạy trong WKWebView = engine Safari của hệ thống. CSS/JS đang dùng cần
  **Safari 16.2+**: `color-mix()` (16.2), `@container` (16.0), `structuredClone` và
  `Array.at` (15.4).
- macOS 11 Big Sur nhận Safari tới 16.6; macOS 10.15 Catalina dừng ở Safari 15.6 —
  giao diện sẽ hỏng màu và bố cục ở đó.
- Apple silicon tối thiểu là 11.0, nên bản universal dùng một mốc chung.
- Big Sur chạy trên hầu hết Mac Intel từ 2013–2014 trở đi (MacBook Air/Pro cuối 2013,
  iMac 2014, Mac mini 2014…). Máy chỉ lên được Catalina thì cần build riêng với UI hạ
  cấp — chưa làm.
- Lưu ý: trên Big Sur phải cập nhật Safari (Software Update) lên 16.x; Safari 14 gốc
  của Big Sur thiếu `color-mix`.

### 5.2 Ký số

Chưa có Apple Developer ID → **ký ad-hoc** (`signingIdentity: "-"`) để Apple silicon
chịu chạy, **không notarize**. Gatekeeper sẽ chặn lần đầu: chuột phải → Open (macOS
11–14), hoặc System Settings → Privacy & Security → **Open Anyway** (macOS 15+), hoặc
`xattr -dr com.apple.quarantine /Applications/NANOFreeDisk.app`.

## 6. Giới hạn và việc tiếp theo

- Notarize + Developer ID (cần tài khoản Apple Developer 99 USD/năm), rồi bật hardened
  runtime.
- Updater cho macOS: build `.app.tar.gz` có chữ ký updater và thêm `darwin-x86_64`,
  `darwin-aarch64` vào `latest.json`.
- Rule pack cho macOS (Clean): `~/Library/Caches/*`, log, Xcode DerivedData /
  Archives / simulator, Homebrew (`brew cleanup`), bản tải dở của Safari… rồi bật lại
  Clean trên macOS.
- Drive health qua IOKit (`smartctl`-like), cảm biến qua SMC, Monitor qua `fs_usage` /
  Endpoint Security.
- `accent_color` (màu nhấn hệ thống) và tên volume bằng `getattrlist`.
- Hỗ trợ Catalina (10.15) nếu cần: build UI riêng không dùng `color-mix`.

## 7. Danh sách kiểm thử thủ công trên Mac Intel

Chuẩn bị: tải artifact `NANOFreeDisk-macos-universal` từ lần chạy CI (hoặc release),
giải nén.

1. **Cài đặt** — mở `.dmg`, kéo NANOFreeDisk vào Applications. Mở app: Gatekeeper chặn
   → làm như §5.2. App mở được, cửa sổ hiện đúng, không lỗi trắng màn hình.
2. **Thanh bên** chỉ có: Home, Analyze, Free up space (DevCache, Duplicates, Old &
   Large, Tidy up), Activity, Settings. **Không** có Clean, Programs, Drive health,
   Hardware, Monitor. ⌘K mở bảng lệnh, ⌘, mở Settings; Settings không có Explorer menu,
   Monitor, Drive health; mục Updates chỉ có nút mở trang Releases.
3. **Ổ đĩa (Home)** — có "Macintosh HD" (hoặc tên ổ của bạn), dung lượng/còn trống gần
   bằng menu Apple → About This Mac → Storage (chênh lệch là "purgeable"). Cắm USB → hiện ở
   Removable; ổ mạng → Network.
4. **Full Disk Access** — lần đầu thẻ "Allow Full Disk Access" hiện ở Home/Analyze/
   Duplicates. Nút mở đúng System Settings → Privacy & Security → Full Disk Access.
   Bật NANOFreeDisk, thoát hẳn (⌘Q) rồi mở lại → thẻ không còn.
5. **Analyze thư mục nhà** (trước khi cấp FDA): macOS hỏi quyền Desktop/Documents/
   Downloads (có câu giải thích của app). Quét xong; `~/Library/Mail`, `Safari`… hiện là
   không đọc được, không treo. Tổng gần bằng `du -sh ~` (sai lệch nhỏ do quyền).
6. **Analyze cả ổ** (sau FDA) — quét "Macintosh HD": tổng ≈ dung lượng đã dùng, không
   bị nhân đôi (`/System/Volumes/Data` hiện là điểm mount, không mở được).
7. **Chặn vùng bảo vệ** — trong Analyze thử xóa: `/Applications/Safari.app`,
   `~/Library/Mobile Documents`, `~/Library/Mail`, một tệp bên trong một `.app`,
   `~/Pictures/Photos Library.photoslibrary`, `~/.claude/projects` (nếu có) → tất cả bị
   từ chối kèm lý do; không gì bị xóa.
8. **Xóa và hoàn tác** — tạo `~/nfd-test/big.bin` (`mkfile 200m ~/nfd-test/big.bin`),
   quét `~/nfd-test`, xóa `big.bin` → nằm trong Thùng rác (Finder, "Put Back" có
   hiện). Activity → Restore → tệp quay lại đúng chỗ.
9. **Duplicates + APFS** — trong Terminal:

   ```bash
   mkdir -p ~/nfd-test/d && cd ~/nfd-test/d
   head -c 5000000 /dev/urandom > a.bin
   cp a.bin b.bin          # bản sao thật
   cp -c a.bin c.bin       # clone APFS (dùng chung block với a.bin)
   ln a.bin d.bin          # hard link: cùng một tệp
   head -c 3000000 /dev/urandom > x.bin
   cp -c x.bin y.bin       # nhóm chỉ có clone
   ```

   Tìm trùng trong `~/nfd-test` (Min size 1 MB): một nhóm `a/b/c` với lãng phí **5 MB**
   (không phải 10 MB); `c.bin` (và/hoặc `a.bin`) có nhãn **APFS clone** và không được
   chọn sẵn; `d.bin` hiện là "tên khác (hard link)" của `a.bin`, không phải bản trùng;
   nhóm `x/y` **không** hiện, phần "đã bỏ qua" có "APFS clones". Xóa `b.bin` → Thùng
   rác; dung lượng trống tăng ~5 MB sau khi dọn Thùng rác.
10. **Bên trong gói** — `ditto /System/Applications/Calculator.app ~/nfd-test/C1.app` và
    `… C2.app`; tìm trùng trong `~/nfd-test`: **không** có tệp nào bên trong `.app`.
11. **iCloud** — trong iCloud Drive, chọn một tệp lớn → "Remove Download" (biểu tượng
    đám mây). Tìm trùng trong thư mục nhà: tệp đó **không** bị tải về (vẫn biểu tượng
    đám mây), phần bỏ qua có "protected"/"online-only".
12. **Thay bằng clone** (Expert) — với nhóm `a/b`, chọn "replace with a link": `b.bin`
    thành clone của `a.bin`, nội dung giữ nguyên (`cmp a.bin b.bin`), Activity → Restore
    đưa bản gốc về.
13. **DevCache** — mở được, liệt kê cache (npm, pnpm, cargo, pip…) nếu có; không xóa gì
    khi chưa xác nhận.
14. **EN/VI, theme** — đổi ngôn ngữ và theme sáng/tối, mọi chữ đều dịch, màu đúng.
15. **Chạy test trên máy** (tuỳ chọn, cần Xcode Command Line Tools + Rust):
    `cargo nextest run --workspace` (một test sẽ chuyển một tệp tạm vào Thùng rác rồi
    đưa về).

Ghi lại kết quả (bước nào sai, ảnh chụp, phiên bản macOS / Safari) vào issue để sửa.
