# Gỡ phần mềm + Leftovers + Rác phần mềm (E1): kế hoạch chi tiết

> Trạng thái: **xong 4 lát cắt** (2026-10-21) — còn kiểm trên máy thật bằng `gui-smoke` và một lần gỡ phần mềm "mọi người dùng" (UAC); CI đã gỡ thật một phần mềm giả per-user, dọn 4 leftovers rồi khôi phục đủ. Mục **E1** ở `docs/PLAN.md` §13.2 (nhóm E) kéo lên làm sớm theo yêu cầu người dùng: tương đương *Uninstall + Leftovers* và *Software Redundant Files* của IObit Uninstaller, nhưng theo đường an toàn của app: `Planner → SafetyGate → Executor → UndoJournal` (`docs/PHASE3_CLEAN.md`). Không có gì bị xóa vĩnh viễn: tệp vào **Quarantine** (hoặc Thùng rác), khóa registry được **xuất ra `.reg`** trước khi xóa, mọi thứ khôi phục được từ Activity.

## 1. Mục tiêu & DoD

Người dùng muốn một chỗ để: (1) xem phần mềm đã cài và dung lượng của chúng, (2) gỡ bằng **trình gỡ chính chủ**, (3) sau khi gỡ, dọn **leftovers** (thư mục, shortcut, khóa registry mà trình gỡ để lại), (4) dọn **rác phần mềm** (cache, log, crash dump, bản cập nhật cũ) của phần mềm *đang cài* mà không ảnh hưởng tới chúng.

**DoD:**
- Danh sách phần mềm: Win32 (registry Uninstall HKLM 64/32 + HKCU) và Store/MSIX; tên, nhà phát hành, phiên bản, dung lượng, ngày cài, nguồn, phạm vi (chỉ tôi / mọi người dùng); ẩn mục hệ thống / bản cập nhật; tìm kiếm, sắp xếp.
- Gỡ: chạy đúng lệnh gỡ của nhà sản xuất (MSI → `msiexec /x {GUID}`), chờ cả tiến trình con (NSIS / Inno chép mình vào Temp rồi thoát), gỡ hàng loạt tuần tự. Không bao giờ gỡ NANOFreeDisk hay WebView2 Runtime từ đây; runtime / driver / thành phần Microsoft có cảnh báo riêng.
- Leftovers: chỉ quét **sau khi** trình gỡ chạy xong và phần mềm không còn trong danh sách (hoặc mục "hỏng" — trình gỡ không còn); mỗi mục có lý do (là thư mục cài đặt, cùng tên, shortcut trỏ vào tệp đã mất…), mức 🟢/🟡, chỉ 🟢 được chọn sẵn; thư mục còn chứa tệp của phần mềm khác / đang được dùng / chứa lịch sử hội thoại AI → bị chặn.
- Rác phần mềm: cache Chromium/Electron, crash dump, log, Temp, bản cập nhật cũ (Squirrel, electron-updater) gom theo phần mềm; không trùng với rule pack sẵn có; không bao giờ đụng Local Storage / IndexedDB / Cookies / hồ sơ.
- Khôi phục: tệp từ Quarantine / Thùng rác, khóa registry bằng `reg import` (HKLM qua UAC).
- Test: ≥ 60 test mới (phân tích lệnh gỡ, chuẩn hóa tên, chấm điểm leftovers, chặn thư mục dùng chung, registry fake, rác phần mềm trên fixture); SafetyGate: `Origin::Leftover` + proptest; `gui-smoke` phủ view bằng fixture (không gỡ gì thật). EN + VI, màu theo token.

## 2. Thành phần

### 2.1 Crate mới `nfd-apps`

| Module | Việc |
|---|---|
| `inventory` | Đọc `…\CurrentVersion\Uninstall` (HKLM 64-bit, HKLM 32-bit / WOW6432Node, HKCU) → `Program`; bỏ `SystemComponent=1`, `ParentKeyName` (bản cập nhật), `ReleaseType` Update/Hotfix; gộp trùng giữa hai view; dung lượng = `EstimatedSize` hoặc đo thư mục cài (đo tối đa N entry). Store: `Get-AppxPackage -PackageTypeFilter Main` (PowerShell, JSON) + tên hiển thị từ `AppxManifest.xml` (`ms-resource:` qua `SHLoadIndirectString`); bỏ `SignatureKind=System`, `NonRemovable` → không gỡ được. |
| `command` | Tách `UninstallString` (có/không ngoặc kép, đường dẫn có dấu cách không ngoặc, `%biến%`, `MsiExec.exe /I{…}` → `/X{…}`, `rundll32`). Trình gỡ không còn trên đĩa → mục **hỏng** (đề nghị dọn leftovers luôn). |
| `names` | Chuẩn hóa tên: bỏ phiên bản, `(x64)`, `64-bit`, `™ ®`, hậu tố công ty (`Inc.`, `Ltd`, `GmbH`…); khóa gọn không dấu cách; danh sách từ chung (`common`, `data`, `cache`, `update`, `microsoft`, `windows`…) không bao giờ đủ để khớp một mình. |
| `leftovers` | Chụp **snapshot** trước khi gỡ (tên, publisher, thư mục cài, thư mục trình gỡ, icon, khóa ARP, mã MSI, tên exe chính, thư mục dữ liệu đã khớp); sau khi gỡ: tệp/thư mục, registry, shortcut, dịch vụ (chỉ thông tin). |
| `junk` | Rác phần mềm theo cấu trúc (§2.5), gán về phần mềm. |
| `runner` (Windows) | `ShellExecuteExW` trình gỡ (UAC do manifest của trình gỡ), chờ cây tiến trình con; Store: `Remove-AppxPackage`. |
| `registry` | Trait `Registry` (thật: `nfd_platform::registry`; test: bộ nhớ). |

### 2.2 `nfd-core` / `nfd-platform`

- **`Origin::Leftover`** (SafetyGate): như `Manual` (người dùng chọn), thêm hai ngoại lệ có kiểm soát: thư mục **bên dưới** `Program Files` / `Program Files (x86)` / `ProgramData` (không bao giờ chính các gốc đó) và shortcut trong Start Menu chung (`ProgramData\Microsoft\Windows\Start Menu\Programs\…`). Vẫn chặn: thư mục con thuộc Windows trong Program Files (`Common Files`, `Windows *`, `WindowsApps`, `ModifiableWindowsApps`, `Internet Explorer`, `Microsoft Update Health Tools`, `Reference Assemblies`, `MSBuild`, `dotnet`, `WindowsPowerShell`, `Uninstall Information`, `PackageManagement`), `ProgramData\Microsoft` (trừ Start Menu Programs), `ProgramData\Package Cache`, mọi template bảo vệ khác, lịch sử hội thoại AI, tệp SYSTEM.
- **`DeleteMode::Registry`** (`registry`): mục là khóa / giá trị registry; `stored` = tệp `.reg` sao lưu; `exec::restore` → `reg import` (HKLM cần quyền admin → báo `needs-admin`).
- `nfd_platform::registry`: đọc khóa / giá trị (chọn view 64/32), thời điểm ghi cuối, xóa cây / giá trị, `export` (`reg.exe export /reg:64|32`), ghi `.reg` cho một giá trị (UTF-16 + BOM), `import`.

### 2.3 Gỡ cài đặt

1. Người dùng chọn một hay nhiều phần mềm → dialog: lệnh sẽ chạy, cảnh báo (runtime / driver), phạm vi ("Mọi người dùng → Windows sẽ hỏi quyền quản trị").
2. Snapshot → chạy trình gỡ (`ShellExecuteExW`, cửa sổ của nhà sản xuất hiện ra) → theo dõi tiến trình và **mọi tiến trình con** (snapshot tiến trình: `ppid` + thời điểm tạo, không nhầm PID tái dùng) tới khi hết; nút "Đã gỡ xong — quét leftovers" / "Dừng chờ".
3. Đọc lại registry: khóa ARP đã mất → **đã gỡ**; còn → **vẫn còn cài** (người dùng hủy) → không quét leftovers (Expert: "Quét bất chấp", mọi mục 🟡, không chọn sẵn).
4. Gỡ hàng loạt: tuần tự, mỗi phần mềm một snapshot; leftovers gom theo phần mềm ở cuối.
5. Không dùng `QuietUninstallString` mặc định (người dùng nên thấy trình gỡ); Expert có tùy chọn "gỡ im lặng nếu nhà sản xuất hỗ trợ".

### 2.4 Leftovers (sau khi gỡ)

**Tìm ở đâu** — máy: `Program Files`, `Program Files (x86)`, `ProgramData`, Start Menu chung, Desktop chung; người dùng: `%APPDATA%`, `%LOCALAPPDATA%`, `%LOCALAPPDATA%\Programs`, `LocalLow`, Start Menu, Desktop, Quick Launch / ghim taskbar. Độ sâu: `<gốc>\<tên>` và `<gốc>\<nhà phát hành>\<tên>`.

| Loại | Điều kiện | Mức |
|---|---|---|
| Thư mục cài đặt còn lại | đúng `InstallLocation` / thư mục trình gỡ / thư mục icon, không phải gốc, không chứa phần mềm khác | 🟢 |
| Thư mục cùng tên | tên chuẩn hóa = tên phần mềm / tên exe chính / tên thư mục cài; đủ "riêng" (≥ 4 ký tự, không phải từ chung) và không khớp phần mềm nào khác còn cài | 🟢 |
| Thư mục nhà phát hành | chỉ khi mọi thứ bên trong đều là leftover được đề xuất (sẽ thành rỗng) | 🟡 |
| Khớp yếu | tên chứa tên phần mềm, hoặc khớp cả phần mềm khác | 🟡 (không chọn sẵn) |
| Shortcut `.lnk` | đích nằm trong thư mục cài / là exe của phần mềm, và **đích không còn**; thư mục Start Menu cùng tên chỉ còn các shortcut đó | 🟢 |
| Khóa registry | `Software\<Publisher>\<Tên>`, `Software\<Tên>` (HKCU, HKLM 64/32); khóa ARP còn sót; `App Paths\<exe>` trỏ vào tệp đã mất; giá trị `Run` trỏ vào tệp đã mất | 🟢 (khóa nhà phát hành: 🟡, chỉ khi sẽ rỗng) |
| Dịch vụ | `ImagePath` trỏ vào thư mục cài | chỉ thông tin + lệnh `sc delete` |

**Bị chặn:** SafetyGate (`Origin::Leftover`); chứa / nằm trong thư mục cài, trình gỡ hay icon của phần mềm **khác** (`other-program`); tiến trình đang chạy từ bên trong (`in-use`); chứa lịch sử hội thoại AI (`contains-history`); quá lớn để đo hết (`too-large`). Không bao giờ: `Software\Microsoft\…` (trừ đúng 3 vị trí ARP / App Paths / Run), `Software\Classes`, `Software\Policies`, `SYSTEM\…`, `WOW6432Node` gốc.

**Thực thi:**
- Mục **người dùng** (hồ sơ của tôi, HKCU): tệp → Quarantine (7 ngày), registry → `.reg` sao lưu rồi xóa. Activity kind `uninstall`.
- Mục **máy** (Program Files, ProgramData, Start Menu / Desktop chung, HKLM): **một** lời nhắc UAC cho cả lô — `nanofreedisk.exe --elevated-leftovers <request.json> <result.json>`: tiến trình elevated một lần, tự kiểm tra lại mọi mục (SafetyGate `Origin::Leftover` + chỉ gốc của máy + khóa HKLM đúng dạng), tệp → **Thùng rác** (khôi phục bằng Explorer hoặc "Khôi phục với quyền quản trị"), khóa → `.reg` trong thư mục dữ liệu của app rồi xóa; ghi kết quả, thoát. Không dùng helper elevated thường trú (helper đó chỉ đọc — `nfd-helper`), không có đường elevation im lặng.
- Khôi phục mục máy: "Khôi phục với quyền quản trị" chạy cùng tiến trình một lần (`restore`): `reg import` tệp `.reg` (chỉ khi mọi khóa trong tệp đúng dạng cho phép) và lấy lại từ Thùng rác.

### 2.5 Rác phần mềm (Software junk)

Duyệt `%APPDATA%`, `%LOCALAPPDATA%`, `LocalLow` (2 cấp: `<app>` và `<vendor>\<app>`) + thư mục cài nằm trong hồ sơ người dùng (Squirrel: `%LOCALAPPDATA%\Discord`…), tìm theo **cấu trúc**:

| Loại | Nhận diện | Mức |
|---|---|---|
| Cache Chromium / Electron | thư mục có `Local State` hoặc (`Preferences` + `Network`) → `Cache`, `Code Cache`, `GPUCache`, `DawnCache`, `DawnGraphiteCache`, `DawnWebGPUCache`, `GrShaderCache`, `ShaderCache` (cả `Default`, `Profile N`, `Partitions\*`) | 🟢 |
| Service Worker CacheStorage | như trên | 🟡 (web app offline có thể mất dữ liệu tạm) |
| Crash dump | `Crashpad\{reports,completed,pending}`, `CrashDumps`, `*.dmp` | 🟢 |
| Log | `logs` / `log` trong thư mục Electron | 🟢; nơi khác 🟡 (một số app lưu nhật ký chat trong `logs`) |
| Temp | `Temp` / `tmp` ngay dưới thư mục app | 🟡 |
| Bản cập nhật cũ | Squirrel: `app-<phiên bản>` cũ (giữ bản mới nhất và bản đang chạy) 🟡, `packages\*.nupkg` trừ bản full mới nhất 🟢, `packages\SquirrelTemp` 🟢; electron-updater: `%LOCALAPPDATA%\<app>-updater` 🟢 | |

Không bao giờ: `Local Storage`, `Session Storage`, `IndexedDB`, `databases`, `Cookies`, `Network`, `blob_storage`, `User Data` / hồ sơ, mọi thứ rule pack đã phủ (`Matcher::rules_matching` + gốc của rule nằm bên trong ứng viên → bỏ, tránh trùng với view Dọn rác), `%LOCALAPPDATA%\Microsoft`, `%APPDATA%\Microsoft`, `Packages`. Gán phần mềm theo tên thư mục (như leftovers); không gán được → nhóm "Ứng dụng khác". `requires_closed` = exe của phần mềm (cảnh báo "đóng X trước"). Thực thi: Planner (`Origin::Rule { level }`, rule id `software.<loại>`) → Quarantine, Activity kind `software-junk`.

### 2.6 CLI

```
nfd programs [list] [--all] [--json]
nfd programs junk [--apply] [--level safe|caution] [--json]
nfd programs uninstall <id|tên> [--json]     # chạy trình gỡ, chờ, in leftovers (không xóa)
nfd programs leftovers <id|tên> [--json]      # mục hỏng (trình gỡ đã mất): leftovers, chỉ đọc
```

### 2.7 GUI — view **Phần mềm** (`programs`, nhóm "Giải phóng dung lượng")

- Tab **Đã cài**: tìm kiếm, lọc (Tất cả / Win32 / Store / Chỉ tôi / Lớn), sắp xếp (tên, dung lượng, ngày cài); mỗi dòng: chữ cái đầu làm biểu tượng, tên, nhà phát hành · phiên bản, dung lượng, ngày cài, nhãn (Store, Mọi người dùng 🛡, Runtime, Driver, Hỏng), "rác X MB"; nút Gỡ; chọn nhiều → "Gỡ N phần mềm".
- Luồng gỡ: dialog xác nhận → thẻ tiến trình ("Đang chạy trình gỡ của X — làm theo cửa sổ của nó") → kết quả → **Leftovers**: nhóm Tệp & thư mục / Registry / Shortcut / Dịch vụ (thông tin); 🛡 = cần quyền quản trị (một lời nhắc); "Xóa leftovers đã chọn (X MB)".
- Tab **Rác phần mềm**: quét, nhóm theo phần mềm, ô chọn nhóm, `SelectionBar`, "Dọn an toàn (chỉ 🟢)", `ConfirmTotals`.
- Activity: kind `uninstall`, `software-junk`; mục registry hiện như khóa; "Khôi phục với quyền quản trị" cho mục máy.
- Bảng lệnh (Ctrl+K): "Gỡ phần mềm", "Rác phần mềm". Expert: lệnh `nfd programs …` tương đương.

## 3. Kiểm thử

- Unit: tách `UninstallString` (≥ 15 dạng), chuẩn hóa tên, lọc ARP (SystemComponent, bản cập nhật), gộp trùng 64/32, chấm điểm leftovers trên fixture (thư mục cài còn sót 🟢, vendor chỉ rỗng mới đề xuất, tên chung không khớp, thư mục của phần mềm khác bị chặn), registry fake (khóa Microsoft không bao giờ, khóa vendor chỉ khi rỗng), shortcut trỏ vào tệp đã mất, rác Electron (Cache 🟢, Local Storage không bao giờ), Squirrel giữ bản mới nhất, tránh trùng rule pack.
- SafetyGate: `Origin::Leftover` cho phép `Program Files\X` nhưng không `Program Files`, `Program Files\Common Files`, `Program Files\WindowsApps\…`, `ProgramData\Microsoft\…` (trừ Start Menu Programs), `Windows\…`; proptest: mọi đường dẫn dưới template bảo vệ của Windows vẫn bị chặn.
- Elevated request: từ chối khóa ngoài danh sách cho phép, đường dẫn hồ sơ người dùng, tệp `.reg` chứa khóa lạ.
- `gui-smoke`: fixture riêng (`window.__nfdProgramsFixture`: danh sách phần mềm giả + thư mục leftovers/rác giả trong thư mục tạm), không chạy trình gỡ thật, không đụng registry thật, quên lịch sử ở cuối.

## 4. Lát cắt

1. **Nền** — `Origin::Leftover`, `DeleteMode::Registry`, `nfd_platform::registry`, test gate.
2. **`nfd-apps`** — inventory, command, names, leftovers, junk (+ registry fake), test.
3. **Thực thi** — runner (gỡ + chờ), elevated một lần, journal / khôi phục, CLI `nfd programs`.
4. **GUI** — view Phần mềm, Activity, bảng lệnh, locales, mock, `gui-smoke`; docs (README, CHANGELOG, PLAN).

## 5. Ngoài phạm vi (v1.x)

- Điểm khôi phục hệ thống trước khi gỡ (cần admin; Windows giới hạn 1 điểm / 24 giờ).
- Theo dõi cài đặt (Install Monitor), gỡ plugin trình duyệt, winget.
- "Leftovers của phần mềm đã gỡ từ lâu" (thư mục mồ côi không thuộc phần mềm nào) — dễ nhầm với ứng dụng portable / công cụ dev.
- Rác trong `ProgramData` (cần admin), xóa dịch vụ / scheduled task.
- Biểu tượng thật của phần mềm (trích `DisplayIcon`).

## 6. Rủi ro

- **Khớp nhầm tên** → xóa dữ liệu của phần mềm khác: chỉ khớp chính xác tên chuẩn hóa đủ riêng; khớp phần mềm khác → chặn; mọi thứ vào Quarantine / Thùng rác / `.reg`.
- **Trình gỡ chạy nền / tự khởi chạy lại**: theo dõi cả cây tiến trình; người dùng luôn có nút "Đã xong".
- **Elevation bởi tài khoản khác** (người dùng chuẩn nhập mật khẩu admin): tiến trình elevated chỉ đụng vị trí của máy (không HKCU, không hồ sơ), Thùng rác là của tài khoản admin → ghi chú trong Activity.
- **Thư mục cài đặt chung** (`InstallLocation` = `C:\Program Files` hay hồ sơ người dùng): SafetyGate chặn gốc; module leftovers bỏ thư mục chứa phần mềm khác.
