# NANOFreeDisk — Kế hoạch phát triển (Development Plan)

> **Phiên bản tài liệu:** 1.0 · **Ngày:** 2026-10-16 · **Tác giả:** Lưu Đức Thành Công ([nanodev.io.vn](https://nanodev.io.vn/))
> **Trạng thái:** Draft → sẽ cập nhật theo từng phase. README công khai bằng tiếng Anh, tài liệu kế hoạch nội bộ bằng tiếng Việt.

---

## 0. Tóm tắt điều hành

**NANOFreeDisk** là phần mềm **Disk Usage / Disk Analyzer** mã nguồn mở, viết bằng **Rust**, ưu tiên **Windows 10/11 x64** trước, thiết kế sẵn lớp trừu tượng để port sang **macOS / Linux** sau.

Năm trụ cột chức năng:

| # | Trụ cột | Mô tả ngắn | Điểm khác biệt |
|---|---------|------------|----------------|
| 1 | **Analyze** | Quét & hiển thị dung lượng (treemap, sunburst, top-N, size-on-disk vs logical) | Quét NTFS siêu nhanh qua MFT, nhận biết hardlink / junction / sparse / compressed / cloud placeholder |
| 2 | **Clean** | Phát hiện & lọc tệp rác, tệp trùng lặp | Rule-engine khai báo (TOML), mọi mục đều có **mức rủi ro + giải thích hậu quả** |
| 3 | **DevCache** | Kiểm soát thư mục tạm của AI/IDE/Agent (Claude, OpenAI Codex, Cursor, Copilot, Gemini CLI, Windsurf, JetBrains, Ollama, …) | Catalog riêng: cái nào xóa được, ảnh hưởng gì, cần đóng app nào, cái nào phải giữ |
| 4 | **Monitor** | Giám sát Disk Activity theo thời gian thực: tiến trình nào đang đọc/ghi, throughput, queue, latency | Dựa trên ETW (Event Tracing for Windows), không cần driver |
| 5 | **Health** | Sức khỏe & chẩn đoán phần cứng: S.M.A.R.T., NVMe log, nhiệt độ, TRIM, lỗi volume, cảnh báo | Diễn giải dễ hiểu, cảnh báo sớm trước khi ổ hỏng |

Ngoài 5 trụ cột, **mục 13** liệt kê bản đồ tính năng mở rộng (Analyze+, Clean+/Privacy, Dev Workspace, Games, Apps & Startup, Health+, UX) để sản phẩm trở thành **Windows Workspace Manager** toàn diện; mỗi mục có phiên bản dự kiến ①②③.

Cam kết **an toàn tuyệt đối**: **không bao giờ** xóa file hệ thống / file đang được dùng / file quan trọng; mặc định xóa vào Recycle Bin hoặc Quarantine; có Undo; có audit log; có dry-run.

Sản phẩm phân phối: **bộ cài thương hiệu riêng** (`NANOFreeDisk-Setup.exe`, viết bằng Rust) **+** bộ cài chuẩn `.msi` (WiX) và `.exe` (NSIS) **+** bản portable `.zip`. CI/CD trên GitHub Actions với job **release-check** trước khi publish.

---

## 1. Mục tiêu & phạm vi

### 1.1 Mục tiêu

1. Thay thế tổ hợp *WizTree + CCleaner + Process Monitor + CrystalDiskInfo* bằng một app nhẹ, mở, dễ hiểu.
2. Giải quyết bài toán mới: máy dev đầy ổ vì cache của AI tools / IDE / package manager mà không ai dám xóa vì không biết hậu quả.
3. Làm "sản phẩm mẫu" cho thương hiệu NANO: chất lượng code, docs, CI, installer đều chuẩn open-source.

### 1.2 Trong phạm vi (v1.0 — Windows)

- Quét ổ/thư mục, treemap/sunburst/list, tìm kiếm & lọc, mở trong Explorer, xóa an toàn.
- Junk cleaner theo rule (Windows system junk + app caches + browser caches + GPU shader cache).
- DevCache Manager (catalog AI/IDE/Agent/package manager/game engine caches).
- Duplicate finder (size → partial hash → full hash BLAKE3), xử lý: xóa / hardlink / di chuyển.
- Disk Activity monitor (per-process I/O, throughput, IOPS, latency, top files đang bị ghi).
- Health (SMART ATA + NVMe, nhiệt độ, TRIM, free-space trend, dự báo đầy ổ, cảnh báo tray).
- CLI `nfd` dùng chung core (scripting, CI, headless).
- Installer thương hiệu + MSI + NSIS + portable, auto-update, winget.

### 1.3 Ngoài phạm vi v1.0 (để sau)

- macOS / Linux GUI (chỉ đảm bảo `nfd-core` build và test được trên 3 OS).
- Kernel driver / minifilter (không làm — rủi ro & ký driver tốn kém).
- Chống phân mảnh, phân vùng, clone ổ.
- Cloud sync / telemetry (không thu thập dữ liệu người dùng).

---

## 2. Quyết định công nghệ

| Hạng mục | Lựa chọn | Lý do | Thay thế đã cân nhắc |
|----------|----------|-------|----------------------|
| Ngôn ngữ | **Rust stable, edition 2024, MSRV 1.85** | An toàn bộ nhớ, song song dễ, binary nhỏ, cross-platform | — |
| GUI app chính | **Tauri 2** + **Svelte 5 + TypeScript** + Vite | UI phong phú (treemap/chart), bundler sẵn MSI/NSIS/DMG/AppImage, updater plugin, tray, cross-platform sau này gần như miễn phí | `egui/eframe` (100% Rust, nhanh, nhưng polish UI & chart tốn công hơn; giữ làm Option B nếu không muốn TS) |
| GUI bộ cài riêng | **egui/eframe** (pure Rust, ~3-5 MB) | Không phụ thuộc WebView2 lúc cài (WebView2 có thể chưa có trên Win10 sạch), khởi động tức thì, dễ brand | NSIS custom page (hạn chế UI) |
| Treemap/Chart | Treemap squarified tự viết (Svelte + Canvas/WebGL); `uPlot` cho realtime graph | Hiệu năng với 100k+ rect | ECharts (nặng hơn) |
| Quét thư mục | `jwalk` (parallel) + fast-path **NTFS MFT** (`ntfs` crate, cần admin) | WizTree-level speed khi có admin, fallback tốt khi không | `walkdir` đơn luồng |
| Hash trùng lặp | `blake3` + `memmap2` | Nhanh nhất, đa luồng | SHA-256 |
| Windows API | `windows` (windows-rs) + `windows-sys` | Chính chủ Microsoft | `winapi` (cũ) |
| ETW | `ferrisetw` | Pure Rust wrapper cho Kernel providers (FileIO, DiskIO, Process) | gọi `krabsetw` qua C++ |
| SMART/Health | `windows` IOCTL (`IOCTL_STORAGE_QUERY_PROPERTY`, `SMART_RCV_DRIVE_DATA`, `IOCTL_ATA_PASS_THROUGH`) + WMI `MSFT_StorageReliabilityCounter` (`wmi` crate) | Không cần smartmontools | gọi `smartctl.exe` (fallback tùy chọn) |
| Recycle Bin / Xóa | `trash` crate (IFileOperation) + Restart Manager API để phát hiện file đang dùng | Chuẩn shell Windows, undo được | `DeleteFileW` trực tiếp (chỉ cho permanent sau xác nhận 2 lớp) |
| Rule engine | TOML rule packs (`rules/*.toml`) + `globset` | Dễ đóng góp, không cần code | Lua/Rhai scripting (quá nặng) |
| Config/State | `directories` + TOML + SQLite (`rusqlite`, bundled) cho lịch sử quét/health | Lịch sử trend, nhanh | JSON file |
| Async | `tokio` (IPC, monitor streams), `rayon` (CPU-bound quét/hash) | Chuẩn | — |
| Logging | `tracing` + `tracing-appender` | Structured, rotate | `log` |
| CLI | `clap` v4 derive + `indicatif` | Chuẩn | — |
| Test | `cargo-nextest`, `insta` (snapshot), `tempfile`, `proptest` cho path-matcher | — | — |
| Bảo mật supply-chain | `cargo-deny`, `cargo-audit`, Dependabot, `cargo-sbom` (CycloneDX) | OSS chuẩn | — |
| Changelog/Version | Conventional Commits + `git-cliff` | Tự động | `release-plz` |
| Installer MSI | **WiX v5** qua Tauri bundler (hoặc `cargo-wix` cho CLI) | Chuẩn doanh nghiệp, GPO deploy | — |
| Installer EXE cơ bản | **NSIS** qua Tauri bundler | Nhẹ, quen thuộc | Inno Setup |
| Ký số | SignPath Foundation (miễn phí cho OSS) hoặc Azure Trusted Signing | Tránh SmartScreen | Không ký (SmartScreen warning) |

**Ghi chú về Option B (all-Rust egui):** nếu sau Phase 2 thấy Tauri/TS làm chậm tiến độ, có thể đổi sang egui vì `nfd-core` hoàn toàn độc lập UI. Quyết định cuối chốt ở cuối Phase 2 (sau prototype treemap).

> **Đã chốt (2026-10-16): giữ Tauri 2 + Svelte 5** — treemap 50k rect: layout ~2,3 ms + vẽ ~15 ms (canvas 2D, gom theo màu), hover chỉ vẽ lớp overlay. Chi tiết & số đo: [`docs/adr/0001-gui-toolkit.md`](adr/0001-gui-toolkit.md). Vì MSRV 1.85, Tauri đang ở 2.11.x (2.12 cần Rust 1.90); gói npm `@tauri-apps/*` pin cùng minor.

---

## 3. Kiến trúc

### 3.1 Sơ đồ tổng quan

Xem `docs/assets/architecture.svg`.

```
┌────────────────────────────────────────────────────────────────────┐
│  apps/                                                             │
│   ├─ nanofreedisk        Tauri 2 desktop app (Svelte UI)           │
│   ├─ nfd-cli             CLI `nfd` (scan/clean/dupes/health/watch) │
│   └─ nanofreedisk-setup  Bộ cài thương hiệu (egui)                 │
├────────────────────────────────────────────────────────────────────┤
│  crates/                                                           │
│   ├─ nfd-core        model, scanner, dedupe, cleaner, planner,     │
│   │                  safety (protected paths, undo, quarantine)    │
│   ├─ nfd-rules       rule-pack loader + matcher + catalog DevCache │
│   ├─ nfd-monitor     Disk activity (ETW) + perf counters           │
│   ├─ nfd-health      SMART / NVMe / volume / trend                 │
│   ├─ nfd-platform    trait Platform { fs, trash, locks, elevation }│
│   │     ├─ windows   (v1)                                          │
│   │     ├─ macos     (v2)                                          │
│   │     └─ linux     (v2)                                          │
│   └─ nfd-ipc         shared DTO (serde) giữa core ↔ UI/CLI         │
├────────────────────────────────────────────────────────────────────┤
│  rules/   *.toml rule packs (windows-system, browsers, ai-tools,   │
│           ide-editors, package-managers, game-engines, …)          │
└────────────────────────────────────────────────────────────────────┘
```

### 3.2 Nguyên tắc kiến trúc

1. **Core không biết UI.** Mọi chức năng chạy được qua CLI → dễ test, dễ script, dễ port.
2. **Platform trait.** Mọi call OS-specific nằm sau `nfd-platform::Platform`; Windows impl dùng `windows-rs`. macOS/Linux impl là stub có test skeleton.
3. **Streaming, không block UI.** Scanner phát `ScanEvent` (Progress, Entry, Done) qua channel; UI nhận batch mỗi 16 ms.
4. **Mọi hành động phá hủy đi qua `Planner`.** UI không bao giờ gọi `delete` trực tiếp; nó tạo `CleanPlan` → `SafetyGate` kiểm tra → `Executor` thực thi + ghi `UndoJournal`.
5. **Rule packs là dữ liệu.** Thêm vendor mới = thêm TOML, kèm test.
6. **Elevation tối thiểu.** App chạy user-mode; chỉ bật helper elevated (cùng binary với flag `--elevated-helper`) khi cần MFT scan, SMART qua IOCTL, ETW, hoặc dọn system junk. Giao tiếp qua named pipe, xác thực token.

### 3.3 Model dữ liệu chính (rút gọn)

```rust
pub struct Entry {
    pub id: EntryId,              // index trong arena
    pub parent: Option<EntryId>,
    pub name: Box<str>,
    pub kind: EntryKind,          // File | Dir | Symlink | Junction | MountPoint | Other
    pub logical: u64,             // byte
    pub on_disk: u64,             // allocation size (cluster-aligned, compressed/sparse aware)
    pub mtime: i64,
    pub attrs: Attrs,             // bitflags: SYSTEM, HIDDEN, REPARSE, COMPRESSED, SPARSE, OFFLINE(cloud)
    pub hardlink_count: u16,
    pub file_id: Option<u128>,    // NTFS FileReferenceNumber → đếm hardlink 1 lần
}

pub struct CleanItem {
    pub path: PathBuf,
    pub rule_id: RuleId,
    pub level: RiskLevel,         // Safe | Caution | Advanced | Protected
    pub size: u64,
    pub impact: String,           // "Mất lịch sử phiên Claude Code; không ảnh hưởng settings"
    pub requires_closed: Vec<ProcessName>,
    pub locked_by: Vec<ProcessInfo>,   // từ Restart Manager
}

pub enum DeleteMode { RecycleBin, Quarantine { ttl_days: u16 }, Permanent }
```

### 3.4 Luồng xóa an toàn (Safety pipeline)

```
UI chọn mục ──▶ Planner.build(items, mode)
                    │
                    ▼
            SafetyGate.check(plan)
             ├─ canonicalize path (resolve junction/symlink/8.3, prefix \\?\)
             ├─ reject nếu khớp ProtectedPaths (hard-coded + user)
             ├─ reject nếu attr SYSTEM trong %SystemRoot% / Program Files
             ├─ reject nếu Authenticode = Microsoft trong system dirs
             ├─ reject nếu là root volume / mount point / reparse target khác volume
             ├─ warn nếu Restart Manager báo đang bị khóa → hiển thị process
             ├─ warn nếu RiskLevel ≥ Caution → yêu cầu tick xác nhận riêng
             └─ Protected → không có nút xóa, chỉ "Hướng dẫn dùng công cụ hệ thống"
                    │
                    ▼
            Executor.run(plan)  (dry_run=true mặc định lần đầu)
             ├─ RecycleBin: IFileOperation (undo bằng Explorer hoặc app)
             ├─ Quarantine: move → %LOCALAPPDATA%\NANOFreeDisk\quarantine\<ts>\ + manifest.json
             └─ Permanent: chỉ cho Safe + xác nhận 2 lớp + gõ "DELETE"
                    │
                    ▼
            UndoJournal (SQLite) + audit.log (tracing) + báo cáo kết quả
```

---

## 4. Thiết kế từng module

### 4.1 Analyze — Scanner

- **Hai chế độ:**
  - *Standard*: walker song song trên pool `rayon`; mỗi thư mục đọc bằng `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` (allocation size, reparse tag, file ID 128-bit lấy thẳng từ index thư mục, không mở từng file — nhanh ~5× so với `jwalk` + mở file), không follow reparse point, đếm hardlink qua `file_id`. (Đã thử `jwalk`: chậm hơn và deadlock khi lồng `par_iter`.)
  - *Fast (admin)*: đọc **$MFT** qua `ntfs` crate trên volume handle (`\\.\C:`), xây cây từ FileReference → nhanh hơn 10-30× trên ổ triệu file. Fallback Standard nếu không phải NTFS / không có quyền.
- **Hỗ trợ:** OneDrive/Cloud placeholder (attr `OFFLINE` / `RECALL_ON_DATA_ACCESS` → đếm 0 on-disk, đánh dấu ☁), NTFS compression, sparse, ADS (tùy chọn), junction (hiển thị ↪ không đếm 2 lần), volume mount point.
- **Incremental rescan:** `notify` (ReadDirectoryChangesW) + USN Journal (admin) để refresh thư mục thay đổi thay vì quét lại toàn bộ.
- **Hiển thị:** Treemap (squarified), Sunburst, Tree list (virtualized), Top 100 files, Extension breakdown, Age heatmap (file cũ > N năm), Filters (size/ext/date/attr/owner).
- **Xuất:** CSV/JSON report; CLI `nfd scan D:\ --top 50 --json`.

### 4.2 Clean — Junk & Rule engine

Định dạng rule (TOML):

```toml
[[rule]]
id          = "anthropic.claude-code.shell-snapshots"
vendor      = "Anthropic"
app         = "Claude Code"
category    = "ai-agent"
platform    = ["windows", "macos", "linux"]
paths       = ["${HOME}/.claude/shell-snapshots/**"]
level       = "safe"                     # safe | caution | advanced | protected
regenerates = true
min_age_days = 1
requires_closed = ["claude.exe", "node.exe"]
impact      = "Snapshot môi trường shell tạm; Claude Code tự tạo lại ở phiên tiếp theo."
docs        = "https://docs.anthropic.com/claude-code"
verified_on = 2026-10-16
```

- **Biến đường dẫn:** `${HOME}`, `${LOCALAPPDATA}`, `${APPDATA}`, `${PROGRAMDATA}`, `${TEMP}`, `${SYSTEMROOT}`, `${USERPROFILE}`; glob `**`, `*`, `{a,b}`.
- **Mức rủi ro (RiskLevel):**
  - 🟢 **Safe** — app tự tạo lại, không mất dữ liệu người dùng (temp, GPU cache, logs cũ, thumbnail).
  - 🟡 **Caution** — mất lịch sử / phải tải lại / chậm lần đầu; cần đóng app; cần tick xác nhận (session history của Claude/Codex/Cursor, model cache, Unity `Library`).
  - 🟠 **Advanced** — có thể ảnh hưởng chức năng, ẩn mặc định, bật trong Settings → Expert (`ProgramData\Package Cache`, `SoftwareDistribution`, Docker vhdx).
  - 🔴 **Protected** — không bao giờ xóa qua app, chỉ hướng dẫn (WinSxS → DISM, hiberfil → `powercfg /h off`, Restore points → vssadmin, `Windows.old` → Storage Sense/Disk Cleanup).
- **Rule packs v1:** `windows-system`, `browsers`, `ai-tools`, `ide-editors`, `package-managers`, `game-engines`, `gpu-shader-cache`, `messaging-apps`, `project-build-artifacts` (quét theo marker: `Cargo.toml`→`target/`, `package.json`→`node_modules/`, `.uproject`→`Intermediate/`, `DerivedDataCache/`, `ProjectSettings/`→`Library/`).
- **Catalog DevCache** chi tiết: xem `docs/DEVTOOL_CACHE_CATALOG.md`.

### 4.3 DevCache Manager (UI riêng)

- Nhóm theo **vendor** → **app** → **thư mục**, mỗi dòng hiện: kích thước, số file, lần sửa cuối, mức rủi ro, **"Nếu xóa thì…"**, app cần đóng (và trạng thái đang chạy hay không), nút *Mở thư mục* / *Thêm vào plan*.
- Nút **"Dọn an toàn (chỉ 🟢)"** một chạm cho toàn bộ vendor.
- Hiển thị **"Giữ lại gì"**: config/auth/settings được đánh dấu 🔒 Keep để người dùng thấy app đã cố tình bỏ qua.
- Phát hiện **thư mục lạ** trong `%USERPROFILE%` / `%APPDATA%` / `%LOCALAPPDATA%` có pattern cache/session/logs mà chưa có rule → gắn 🟡 "Chưa xác minh" + link đóng góp rule.
- Có thể **đặt lịch** / nhắc khi vendor vượt ngưỡng (ví dụ `.ollama/models` > 50 GB).

### 4.4 Duplicate finder

- Pipeline: group by size → loại size < ngưỡng → hash 4 KB đầu + 4 KB cuối (BLAKE3) → full hash (mmap, rayon) → so sánh byte nếu bật *paranoid*.
- Bỏ qua: hardlink cùng `file_id`, reparse, cloud placeholder, Protected paths.
- Hành động: giữ bản mới nhất / cũ nhất / trong thư mục ưu tiên; xóa vào Recycle Bin; **tạo hardlink** (cùng volume NTFS) để giữ nguyên đường dẫn nhưng tiết kiệm dung lượng; xuất danh sách.
- Tương tự ảnh (perceptual hash) — **v1.1**.

### 4.5 Monitor — Disk Activity

- **ETW** session `NT Kernel Logger` providers: `FileIo` (Read/Write/Create/Delete), `DiskIo`, `Process`. Cần admin → chạy trong helper elevated, stream kết quả qua pipe.
- Không admin: fallback `GetProcessIoCounters` per-process (delta/giây) + PDH counters (`\PhysicalDisk(*)\Disk Bytes/sec`, `Avg. Disk Queue Length`, `Avg. Disk sec/Transfer`, `% Idle Time`).
- UI: bảng tiến trình (R/B/s, W/B/s, IOPS, file đang thao tác), biểu đồ realtime 60 s / 10 phút, top files đang bị ghi, nút "Jump to folder", cảnh báo khi một tiến trình ghi liên tục > X MB/s trong Y phút (phát hiện log spam, indexer, antivirus).
- Lịch sử 24 h lưu SQLite (downsample).

### 4.6 Health — Chẩn đoán phần cứng

- **Liệt kê ổ:** `IOCTL_STORAGE_QUERY_PROPERTY` (model, serial, firmware, bus NVMe/SATA/USB/SD, rotation, TRIM support), WMI `MSFT_PhysicalDisk` + `MSFT_StorageReliabilityCounter` (wear, temperature, read/write errors, power-on hours).
- **SMART ATA:** `SMART_RCV_DRIVE_DATA` IOCTL → attributes (05 Reallocated, C5 Pending, C6 Uncorrectable, C7 CRC, C2 Temp, 09 POH, F1/F2 LBAs written). Diễn giải: Good / Caution / Bad theo ngưỡng kiểu CrystalDiskInfo.
- **NVMe:** `StorageDeviceProtocolSpecificProperty` → SMART/Health log page 02h (Percentage Used, Available Spare, Temp, Data Units Written, Unsafe Shutdowns, Media Errors).
- **USB bridge:** thử SAT (SCSI-ATA Translation) pass-through; nếu không được → báo "không hỗ trợ qua cầu USB".
- **Volume:** dirty bit (`FSCTL_IS_VOLUME_DIRTY`), free-space trend 30 ngày → dự báo ngày đầy ổ, BitLocker status, TRIM enabled (`DisableDeleteNotify`), cluster size.
- **Cảnh báo:** tray notification khi SMART đổi trạng thái, nhiệt độ > ngưỡng, free < 10 %.
- **Báo cáo:** export JSON/HTML để gửi hỗ trợ kỹ thuật.

### 4.7 CLI `nfd`

```
nfd scan <path> [--fast] [--top N] [--json|--csv] [--min-size 10MB]
nfd clean [--rules ai-tools,browsers] [--level safe] [--dry-run] [--mode recycle|quarantine]
nfd devcache [list|clean --vendor anthropic --level safe]
nfd dupes <path...> [--min-size 1MB] [--action list|recycle|hardlink]
nfd watch [--interval 1s] [--top 20]
nfd health [--json] [--watch]
nfd quarantine [list|restore <id>|purge --older 7d]
nfd rules [list|validate <file>]
```

---

## 5. An toàn: "không bao giờ xóa nhầm"

### 5.1 Danh sách Protected (hard-coded, không tắt được)

- `%SystemRoot%\**` (trừ các rule explicit: `Temp`, `Logs\CBS\*.log` cũ, `SoftwareDistribution\Download` ở mức Advanced, `Minidump`, `MEMORY.DMP`).
- `%ProgramFiles%`, `%ProgramFiles(x86)%`, `%ProgramData%\Microsoft\**`, `%ProgramData%\Package Cache` (Advanced only).
- `System Volume Information`, `$Recycle.Bin` (chỉ qua API Empty Recycle Bin), `$MFT`, `$LogFile`, `$Extend`, `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`, `DumpStack.log.tmp`.
- `Windows\WinSxS`, `Windows\Installer`, `Windows\servicing`, `Windows\System32`, `SysWOW64`, `Boot`, `EFI`, `Recovery`.
- Thư mục user data: `Documents`, `Desktop`, `Pictures`, `Videos`, `Music`, `Downloads` (chỉ được xóa khi **người dùng chọn tay từng file** trong Analyze, không bao giờ qua rule).
- Config/credential của dev tools: `~/.claude.json`, `~/.claude/settings*.json`, `~/.codex/auth.json`, `~/.codex/config.toml`, `~/.gemini/oauth_creds.json`, `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.azure`, `~/.kube`, `~/.docker/config.json`, `~/.npmrc`, `~/.cargo/credentials*`, `~/.git-credentials`, `.env*`.
- Mọi đường dẫn là **root volume**, **mount point**, hoặc **target của junction/symlink nằm ngoài volume**.

### 5.2 Kiểm tra động

- **Restart Manager** (`RmStartSession/RmRegisterResources/RmGetList`): biết chính xác process nào đang giữ file; app **không bao giờ kill** process, chỉ hiển thị và cho người dùng tự đóng.
- **Authenticode** (`WinVerifyTrust`): file ký bởi Microsoft trong system dirs → Protected.
- **Attribute SYSTEM** ngoài rule explicit → Protected.
- **Size sanity:** plan xóa > 20 % dung lượng ổ hoặc > 50 GB → cảnh báo đỏ + xác nhận thêm.
- **Path canonicalization** trước mọi so khớp (chống bypass qua `..`, 8.3 name, junction, `\\?\`, case).

### 5.3 Phục hồi

- Mặc định **Recycle Bin**; **Quarantine** cho thư mục cache lớn (Recycle Bin chậm với 100k file) — move cùng volume (rename, tức thì), TTL 7 ngày, auto purge, có nút *Restore*.
- **Undo Journal** (SQLite): mọi thao tác có id, danh sách đường dẫn gốc, mode, thời gian; `nfd quarantine restore <id>`.
- **System Restore Point** đề xuất trước khi dọn Advanced system junk (`SystemRestore.CreateRestorePoint` qua WMI, cần admin).
- **Audit log** `%LOCALAPPDATA%\NANOFreeDisk\logs\audit-*.log` (không bao giờ tự xóa trong 90 ngày).

### 5.4 Kiểm thử an toàn (bắt buộc trước release)

- Test suite `safety_gate`: 200+ case path (junction vào `C:\Windows`, symlink ra ổ khác, `..\..`, 8.3 `PROGRA~1`, UNC, `\\?\`, case khác nhau, trailing dot/space, ADS `file.txt:stream`).
- Property test: với mọi path sinh ngẫu nhiên dưới Protected root → `SafetyGate` phải reject.
- Integration test trên VM Windows sạch (`windows-latest`): chạy `nfd clean --level safe --dry-run` toàn hệ thống → kiểm tra **không có** mục nào nằm trong Protected list.
- Fuzz rule loader (`cargo-fuzz`) để TOML lỗi không crash.

---

## 6. Installer & phân phối

### 6.1 Bốn gói phát hành mỗi version

| Gói | Tên file | Công nghệ | Mục đích |
|-----|----------|-----------|----------|
| **Bộ cài thương hiệu** | `NANOFreeDisk-Setup-x.y.z-x64.exe` | Rust + egui, payload zstd nhúng | Trải nghiệm cài đặt mang thương hiệu NANO (splash, màu, font, wizard) |
| MSI chuẩn | `NANOFreeDisk-x.y.z-x64.msi` | WiX v5 (Tauri bundler) | IT/doanh nghiệp, GPO, Intune, silent `/qn` |
| EXE chuẩn | `NANOFreeDisk-x.y.z-x64-setup.exe` | NSIS (Tauri bundler) | Người dùng quen kiểu cài truyền thống, `/S` silent |
| Portable | `NANOFreeDisk-x.y.z-x64-portable.zip` | zip + `portable.flag` | Không cài, config nằm cạnh exe |

Kèm: `SHA256SUMS.txt`, `SHA256SUMS.txt.minisig` (minisign), `sbom.cdx.json`, `CHANGELOG` trích đoạn trong Release notes.

### 6.2 Bộ cài thương hiệu (`apps/nanofreedisk-setup`)

- **Màn hình:** Splash (logo NANO, tagline) → Welcome → License (MIT) → Install type (Chỉ tôi / Mọi người – cần UAC) → Thư mục & tùy chọn (Start Menu, Desktop shortcut, context-menu "Analyze with NANOFreeDisk", khởi động cùng Windows cho tray monitor, file association `.nfdreport`) → Tiến trình (giải nén zstd, kiểm tra & cài WebView2 Evergreen nếu thiếu, ghi registry) → Hoàn tất (Launch, mở README, QR donate nhỏ góc dưới nếu bật).
- **Registry:** `HKCU|HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\NANOFreeDisk` (DisplayName, DisplayVersion, Publisher, InstallLocation, UninstallString, DisplayIcon, EstimatedSize, URLInfoAbout=nanodev.io.vn).
- **Uninstaller:** cùng binary với `--uninstall`, hỏi có giữ config/quarantine không.
- **Silent:** `/S /D=<dir> /ALLUSERS` tương thích tham số NSIS để script dễ dùng.
- **Tự cập nhật:** app dùng `tauri-plugin-updater` kéo manifest từ GitHub Releases (`latest.json`), ký bằng minisign key; portable chỉ thông báo.
- **Ký số:** ưu tiên SignPath Foundation (miễn phí OSS) hoặc Azure Trusted Signing; nếu chưa có → README ghi rõ cách verify SHA256 + minisign.

### 6.3 Kênh khác

- `winget install NANODev.NANOFreeDisk` (manifest tự tạo PR qua `winget-releaser` action sau release).
- Scoop bucket (tùy chọn), Chocolatey (tùy chọn).
- `cargo install nfd-cli` cho CLI.

---

## 7. CI/CD & Release

### 7.1 `ci.yml` (push/PR)

| Job | Nội dung |
|-----|----------|
| `fmt` | `cargo fmt --all --check` |
| `clippy` | `cargo clippy --workspace --all-targets -- -D warnings` (Windows) |
| `test` | `cargo nextest run --workspace` trên `windows-latest`; core crates thêm `ubuntu-latest`, `macos-latest` |
| `deny` | `cargo deny check` (licenses, advisories, bans, sources) |
| `rules` | `nfd rules validate rules/` — mọi TOML hợp lệ, không rule nào khớp Protected path |
| `docs` | link-check README/docs (`lychee`) |
| `frontend` | `pnpm check` (svelte-check, `--fail-on-warnings`) + `pnpm test` (layout/theme) + `check-locales.mjs` (EN/VI đủ key) + `pnpm build` (từ Phase 2) |

### 7.2 `release.yml` (tag `v*.*.*`)

1. **release-check** (chặn nếu fail):
   - version trong `Cargo.toml` workspace, `tauri.conf.json`, `package.json` == tag;
   - `CHANGELOG.md` có mục cho version;
   - `cargo audit` không có lỗ hổng critical;
   - rule packs validate; fmt/clippy/test xanh.
2. **build-windows**: build app (Tauri → MSI + NSIS), CLI, custom setup; chạy **smoke test**: cài silent → `nfd --version` → `nfd scan %TEMP% --json` → gỡ silent; ký số nếu có secret.
3. **build-core-crossplatform**: `nfd-cli` cho `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` (chỉ core/CLI, đảm bảo tương lai).
4. **checksums + sbom + minisign**.
5. **changelog**: `git-cliff --latest` → body Release.
6. **publish**: tạo GitHub Release (draft/prerelease nếu tag có `-rc`/`-beta`), upload asset, cập nhật `latest.json` cho updater.
7. **post-release**: PR winget manifest; cập nhật badge/website.

### 7.3 Versioning & nhánh

- SemVer; nhánh `main` luôn release-able; feature branch → PR → squash merge; Conventional Commits (`feat:`, `fix:`, `safety:`, …); commit `safety:` bắt buộc có test.
- Tag `v0.x` trong lúc phát triển; `v1.0.0` khi đủ 5 trụ cột + installer + docs.

---

## 8. Lộ trình (solo dev, ~18 tuần tới v1.0)

| Phase | Tuần | Mục tiêu | Definition of Done |
|-------|------|----------|--------------------|
| **0 – Foundation** | 1 | Repo, workspace, CI, brand assets, quyết định công nghệ | CI xanh; README/CONTRIBUTING/LICENSE; `nfd --version` chạy |
| **1 – Core Scanner + CLI** | 2–4 | `nfd-core` scanner (standard + MFT fast), model arena, `nfd scan` JSON/CSV, benchmark | Quét `C:\` 1 triệu file < 10 s (fast) / < 60 s (standard); test hardlink/junction |
| **2 – GUI Analyze** ✅ | 5–7 | Tauri app: drive overview, treemap/sunburst/list, filter/search, mở Explorer, xóa thủ công vào Recycle Bin | Demo video; 60 fps treemap 50k rect; chốt Tauri vs egui — **xong 2026-10-16** |
| **3 – Safety + Clean** | 8–9 | SafetyGate, Planner/Executor, Quarantine, Undo, rule engine, packs `windows-system`, `browsers`, `gpu-shader-cache` | 200+ safety tests xanh; `nfd clean --dry-run` |
| **4 – DevCache Manager** | 10 | Catalog + packs `ai-tools`, `ide-editors`, `package-managers`, `game-engines`, UI DevCache | Catalog ≥ 60 rule có impact text; phát hiện thư mục lạ |
| **5 – Duplicates** | 11–12 | Dedupe pipeline, UI, hardlink action | 100 GB ảnh/video trong < 5 phút (SSD) |
| **6 – Monitor** | 13–14 | ETW helper elevated, per-process I/O, realtime chart, cảnh báo | Overhead < 2 % CPU |
| **7 – Health** | 15–16 | SMART ATA/NVMe, WMI reliability, trend, tray alert, export | Khớp CrystalDiskInfo trên ≥ 5 ổ mẫu (SATA SSD, NVMe, HDD, USB) |
| **8 – Installers** | 17 | Custom setup (egui), MSI, NSIS, portable, updater, ký số | Cài/gỡ sạch trên VM Win10/11; silent OK |
| **9 – Release 1.0** | 18 | Screenshots thật, docs, website, winget, release-check | `v1.0.0` trên GitHub Releases |
| **10 – Cross-platform** | sau 1.0 | macOS (IOKit SMART, FSEvents, `fs_usage`), Linux (`/proc/diskstats`, fanotify, libatasmart/smartctl), bundler DMG/AppImage/deb | `nfd` CLI đủ 3 OS → GUI |
| **11 – v1.x** | — | Perceptual dedupe ảnh, scheduled clean, theme tùy chỉnh, i18n (vi/en/…), rule packs từ community | — |

**Thứ tự ưu tiên nếu thiếu thời gian:** 1 → 3 → 4 → 2 → 8 → 7 → 5 → 6 (an toàn & DevCache là điểm bán hàng; Monitor khó nhất để sau).

> **Cập nhật:** sau khi thêm các tính năng ① ở mục 13 (Health Score, What-changed, Folder Encyclopedia, Repo Finder, Toolchain Inventory, …), tổng thời gian v1.0 ước tính **~21 tuần**; Phase 2 kéo dài thành 4 tuần, Phase 4 thành 3 tuần. Các tính năng ② và ③ xếp vào v1.x / v2.0 theo mục 13.4.

---

## 9. Cấu trúc repo mục tiêu

```
NANOFreeDisk/
├─ .github/              workflows (ci, release), issue/PR templates, dependabot, FUNDING
├─ apps/
│  ├─ nanofreedisk/      Tauri 2 app  (src-tauri/ + src/ Svelte)
│  ├─ nfd-cli/           CLI
│  └─ nanofreedisk-setup/ bộ cài thương hiệu (egui)
├─ crates/
│  ├─ nfd-core/ nfd-rules/ nfd-monitor/ nfd-health/ nfd-platform/ nfd-ipc/
├─ rules/                *.toml rule packs (data, có test)
├─ docs/
│  ├─ PLAN.md  DEVTOOL_CACHE_CATALOG.md  SAFETY.md  ARCHITECTURE.md
│  └─ assets/  banner.svg architecture.svg ui-*.svg screenshots/ qr-bidv-donate.jpg
├─ installer/            WiX fragments, NSIS hooks, icons (.ico), branding
├─ scripts/              bump-version.ps1, make-checksums.ps1, smoke-test.ps1
├─ Cargo.toml (workspace)  rust-toolchain.toml  deny.toml  rustfmt.toml  cliff.toml
├─ README.md  CHANGELOG.md  CONTRIBUTING.md  SECURITY.md  CODE_OF_CONDUCT.md  LICENSE
└─ .gitignore  .gitattributes  .editorconfig
```

---

## 10. Rủi ro & biện pháp

| Rủi ro | Mức | Biện pháp |
|--------|-----|-----------|
| Xóa nhầm file quan trọng → mất uy tín | Cao | SafetyGate hard-coded + Quarantine mặc định + property test + review bắt buộc cho `safety:` commit |
| SMART qua USB/RAID không đọc được | Trung | Báo rõ "không hỗ trợ", fallback `smartctl` nếu user cài |
| ETW cần admin, antivirus chặn | Trung | Helper elevated riêng, fallback IoCounters; ký số binary |
| SmartScreen cảnh báo installer chưa ký | Trung | SignPath OSS; hướng dẫn verify; winget giúp tăng reputation |
| Tauri/TS làm chậm solo dev | Trung | Core độc lập UI; checkpoint cuối Phase 2 để đổi sang egui |
| Rule sai (vendor đổi đường dẫn) | Trung | Mỗi rule có `verified_on` + test fixture; cộng đồng PR; thư mục lạ mặc định 🟡 |
| Scope creep | Cao | DoD rõ từng phase; v1.0 chỉ Windows |

---

## 11. Quy ước chất lượng

- `#![deny(unsafe_op_in_unsafe_fn)]`, mọi `unsafe` (FFI) phải có `// SAFETY:` comment; FFI gom trong `nfd-platform`.
- `clippy::pedantic` bật chọn lọc; `-D warnings` trên CI.
- Test coverage mục tiêu: `nfd-core::safety` 100 %, còn lại ≥ 70 %.
- Không telemetry, không network trừ update check (có thể tắt) và link docs.
- **Ngôn ngữ (bắt buộc, mọi phiên bản):** luôn có đủ **EN và VI** cho toàn bộ UI, CLI, installer, tray, thông báo và text "Nếu xóa thì…" trong rule packs (`impact` + `impact_vi`). Chuỗi qua `fluent` (`locales/en/*.ftl`, `locales/vi/*.ftl`); CI chặn PR nếu một key có ở EN mà thiếu ở VI (hoặc ngược lại). Tự chọn theo ngôn ngữ Windows, đổi được trong Settings không cần khởi động lại. Thêm ngôn ngữ khác qua PR.
- **Theme (bắt buộc, mọi phiên bản):** 4 chế độ **Light / Dark / System / Custom**. *System* theo `prefers-color-scheme` + accent color của Windows. *Custom* cho người dùng tự đặt màu (nền, surface, text, accent, 4 màu risk 🟢🟡🟠🔴, bảng màu treemap) qua color picker, xem trước trực tiếp, lưu/xuất/nhập `theme.toml`, có vài preset sẵn (NANO, Nord, Solarized, High-contrast). Mọi màu đi qua design tokens (CSS variables / egui `Visuals`) — không hard-code màu trong component; đổi theme không cần khởi động lại; bộ cài thương hiệu cũng theo Light/Dark của hệ thống.
- Accessibility: keyboard navigation, high-contrast, font scaling, kiểm tra tương phản WCAG AA cho theme Custom (cảnh báo nếu người dùng chọn màu khó đọc).

---

## 12. Việc cần làm ngay (Phase 0 checklist)

- [x] Workspace Cargo + crates stub + CLI `nfd --version`
- [x] `.gitignore`, `.gitattributes`, `.editorconfig`, `rustfmt.toml`, `deny.toml`, `rust-toolchain.toml`
- [x] README (EN) + banner/diagram SVG + QR donate
- [x] LICENSE MIT, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, issue/PR templates, FUNDING
- [x] `ci.yml`, `release.yml` (release-check + build + checksums + publish)
- [x] Catalog DevCache v0 (`docs/DEVTOOL_CACHE_CATALOG.md`) + rule pack mẫu `rules/ai-tools.toml`
- [ ] `git init`, push `congluuit/NANOFreeDisk`, bật branch protection, Dependabot
- [x] Thiết kế logo/icon `.ico` (256/128/64/48/32/24/16) + chốt màu thương hiệu (`docs/BRAND.md`, master `docs/assets/brand/icon-1024.png`)
- [x] Prototype treemap → chốt **Tauri 2 + Svelte 5** (`docs/adr/0001-gui-toolkit.md`)
- [ ] Đăng ký SignPath Foundation (cần repo public + 1 release)

**Phase 1 — Core Scanner + CLI**

- [x] `nfd-platform::fsinfo`: `read_dir` (liệt kê kèm metadata bằng `FileIdExtdDirectoryInfo`, fallback `std::fs::read_dir` cho FAT/SMB) + `file_info` (handle `FILE_READ_ATTRIBUTES` + `OPEN_REPARSE_POINT`; phân biệt junction / mount point / symlink; kích thước nén cho NTFS-compressed, sparse, WOF)
- [x] `nfd-core::scanner` *standard mode*: walker song song (rayon), arena pre-order `Vec<Entry>`, không bao giờ follow link hay vượt volume, khử trùng hardlink theo `file_id` (bắt cả bind-filter projection của app MSIX), lỗi (access denied) gom thành `ScanIssue` thay vì dừng, `ScanEvent::{Progress, Done}` qua channel, cờ hủy
- [x] Tổng hợp: `roll_up` kích thước thư mục (logical + on_disk), `ChildIndex`, top-N file, thống kê theo extension
- [x] CLI `nfd scan <path> [--top N] [--json|--csv] [--min-size 10MB] [--threads N] [--fast] [--exact-links]` + spinner `indicatif` trên stderr; JSON schema `nanofreedisk.scan.v1` (`docs/schema/scan-v1.schema.json`)
- [x] Test: cây lồng nhau, cặp hardlink, junction trong/ngoài cây, symlink file, root là junction, thư mục không đọc được (Unix), progress/cancel, schema JSON, CSV
- [x] Đo thực tế: `%LOCALAPPDATA%` 1,01 triệu mục (844k file, 165k thư mục) trong ~3,6 s (standard, 16 luồng, SSD); `--exact-links` ~21 s
- [ ] *Fast mode* đọc `$MFT` (feature `mft`, hiện là stub → fallback standard)
- [ ] Benchmark `C:\` ≥ 1 triệu file + so sánh với WizTree/WinDirStat
- [ ] Incremental rescan (`notify` / USN Journal)

**Phase 2 — GUI Analyze** ✅ **hoàn tất 2026-10-16**

- [x] App Tauri 2 + Svelte 5 + TS (`apps/nanofreedisk`): sidebar (module tương lai hiện "P3…P7"), Home = 3 nút lớn (G1, nút Clean/Health chờ phase sau) + thẻ ổ đĩa (dung lượng, FS, loại, cảnh báo < 20 % / < 10 % trống), "Quét một thư mục…"
- [x] Quét nền + tiến trình realtime (`scan-progress` / `scan-done`), hủy được; `nfd-platform::volumes`
- [x] Analyze: breadcrumb, lên cấp (Backspace), tree list ảo hóa (mở rộng lười, phím mũi tên/Enter/Delete), **treemap squarified** (canvas, nhãn, hover tooltip, double-click để zoom, gom "mục nhỏ"), **sunburst** 6 vòng, chú giải màu theo loại tệp, tab Tệp lớn nhất / Loại tệp / Kết quả tìm kiếm / Không đọc được, panel chi tiết, menu chuột phải
- [x] Tìm kiếm & lọc (A4 cơ bản): tên (chuỗi con hoặc glob `*.iso`), loại, min/max size, phần mở rộng, thời gian sửa, ẩn hidden/system, chỉ cloud — `nfd-core::query`, Ctrl+F
- [x] Mở trong Explorer / Mở / Sao chép đường dẫn; xuất báo cáo JSON (`nanofreedisk.scan.v1`) / CSV
- [x] **Xóa thủ công vào Recycle Bin**: `nfd-core::safety::check_manual_delete` (chặn gốc ổ, `%SystemRoot%`, Program Files, credential, `$Recycle.Bin`/`System Volume Information`… trên mọi ổ, và chặn chính các thư mục user/AppData/Temp nhưng cho xóa từng mục bên trong), chặn link/junction/mount point, thư mục chứa mount point; kiểm tra lại trên filesystem thật ngay trước khi xóa; dialog hiện lý do bị chặn, xác nhận thêm khi > 50 GB hoặc > 20 % ổ (§5.2); `nfd-platform::trash` dùng `SHFileOperationW` + `FOF_ALLOWUNDO` + `FOF_WANTNUKEWARNING` (không bao giờ xóa vĩnh viễn âm thầm); cây/tổng dung lượng cập nhật sau khi xóa (`ScanTree::remove`)
- [x] EN/VI qua Fluent (`locales/{en,vi}/app.ftl`), `scripts/check-locales.mjs` chặn thiếu key / lệch biến; đổi ngôn ngữ không cần khởi động lại
- [x] Theme Light / Dark / System (theo Windows + màu accent từ registry) / Custom (23 token, color picker, preset NANO Dark/Light, Nord, Solarized, High-contrast, nhập/xuất `theme.toml`, cảnh báo WCAG AA); cỡ chữ 85–140 %
- [x] Đo hiệu năng treemap 50k rect (ADR 0001); mock backend cho dev/preview trong browser; `scripts/gui-smoke.mjs` chạy exe thật qua WebView2 DevTools
- [x] **Old & Large (A2)** — `nfd_core::insights::old_and_large`, một lượt trên arena bằng `query::Filter`: tệp ≥ N GB, tệp ≥ 100 MB không sửa > N năm (Windows không ghi `atime` tin cậy → dùng `mtime`, UI nói rõ), bộ cài / ảnh đĩa (`.exe .msi .msix .appx .iso .img .zip .7z .rar`) trong mọi thư mục `Downloads` cũ hơn 30 ngày; số lượng/tổng luôn đầy đủ dù chỉ trả top-N, kèm tổng "không trùng" giữa 3 danh sách. View "Cũ & lớn": ngưỡng chỉnh được (Expert thêm ngưỡng size tệp cũ và tuổi bộ cài), chọn nhiều → Recycle Bin qua `check_manual_delete` như Analyze
- [x] **Thư mục rỗng & shortcut hỏng (A3, chỉ xem)** — `insights::empty_folders` (chỉ liệt kê cây rỗng ngoài cùng; thư mục không đọc được không bao giờ bị coi là rỗng; thư mục mà safety chặn — Windows, Program Files, Documents… — không liệt kê nhưng thư mục rỗng bên trong vẫn có); `nfd_core::shortcut` đọc `.lnk` theo MS-SHLLINK (LinkInfo local/UNC, relative path, `EnvironmentVariableDataBlock` + `%VAR%`), không qua shell, không chạm ổ mạng / ổ quang; chỉ báo hỏng khi chắc chắn (ANSI không phải ASCII, shell item, MSI advertised → "không kiểm tra"); phân biệt "mất tệp đích" và "ổ không kết nối". Dọn/xóa làm ở Phase 3 (Quarantine + Undo)
- [x] **Báo cáo HTML (A6)** — `src/lib/report.ts`: một tệp tự chứa, không script, không tài nguyên ngoài, treemap SVG tĩnh + bảng mục lớn nhất / tệp lớn nhất / loại tệp / tóm tắt Old & Large, màu là CSS variables của theme đang dùng, EN/VI; lệnh `save_html_report` chỉ ghi `*.html`
- [x] **Simple / Expert (G2)** — Simple ẩn bộ lọc nâng cao, thuộc tính, tab "Không đọc được", xuất JSON/CSV; Expert hiện tất cả + lệnh `nfd` tương đương (quét, top, `--min-size`) có nút sao chép; chuyển ở sidebar, Settings hoặc Ctrl+K
- [x] **Command palette Ctrl+K (G3)** — view, hành động (quét thư mục, quét lại, báo cáo HTML, đổi chế độ), ổ đĩa, thư mục trong lần quét (tìm trên index trong RAM khi gõ), cài đặt (ngôn ngữ, theme, cỡ chữ); khớp mờ bỏ dấu tiếng Việt ("cai dat" → "Cài đặt"), phím ↑/↓/Enter/Esc
- [x] `scripts/check-locales.mjs` kiểm tra thêm mọi key `t("…")` / `label: "…"` mà UI dùng đều có trong `en/app.ftl`; `gui-smoke.mjs` phủ Old & Large, thư mục rỗng, shortcut hỏng, `save_html_report`, view mới và Ctrl+K
- [x] **Lịch sử quét** — crate `nfd-history` (SQLite qua `rusqlite` 0.40 `bundled`, build được trên MSRV 1.85; `cargo deny` licenses/bans/sources ok). DB ở `%LOCALAPPDATA%\vn.io.nanodev.nanofreedisk\history.db` (app local data dir của Tauri). Mỗi lần quét ghi 1 snapshot trước `scan-done`: tổng, dung lượng/trống của ổ, thư mục ≥ 1 MB + 2 cấp đầu (tối đa 40k), tệp ≥ 10 MB + top 500 (tối đa 5k); mỗi snapshot nhớ ngưỡng đầy đủ (`dir_floor`, `file_floor`) để diff không báo "mất" thứ chỉ là không được lưu. Giữ 30 snapshot / thư mục và 300 tổng; DB hỏng → đổi tên `*.broken-<ts>` rồi tạo mới; DB của bản mới hơn → để nguyên, chạy không lịch sử. Settings: bật/tắt, số snapshot, xóa lịch sử (có xác nhận). Đo: ghi snapshot cây 1 triệu entry ~0,1 s (release)
- [x] **"What changed?" (A1)** — `History::diff`: thư mục tăng/giảm (chỉ hiện thư mục cụ thể nhất: bỏ thư mục cha khi một thư mục con giải thích ≥ 80 % thay đổi), tệp lớn mới/to ra và mất/nhỏ đi, Δ tổng, Δ số tệp, Δ dung lượng trống của ổ (cả thay đổi ngoài vùng quét). Tab "Có gì thay đổi?" trong Analyze: chọn lần quét để so, bấm một dòng để nhảy tới thư mục/tệp (`find_path`)
- [x] **Health Score (F1)** — `nfd_core::health::score` (thuần, có test): trừ điểm khi ổ còn < 20 / 10 / 5 % trống (ổ hệ thống nặng hơn), khi ổ sẽ đầy trong < 90 / 30 ngày (dự báo F9 = hồi quy tuyến tính trên dung lượng trống trong lịch sử 90 ngày, `History::forecast`), khi Old & Large chiếm ≥ 3 / 10 % ổ; mục "chưa quét" / "chưa đủ lịch sử" chỉ để thông tin. Trang chủ: vòng điểm + 3 lý do lớn nhất, mỗi lý do có nút Fix
- [x] **CLI `nfd diff <root> [--since 7d] [--json] [--min-delta] [--top] [--list] [--db]`** đọc cùng DB với app (`nfd_history::default_path`), `nfd scan --save` ghi snapshot từ dòng lệnh; báo cáo HTML có mục "Thay đổi kể từ …" so với lần quét trước
- [x] **Folder Encyclopedia (A5)** — `rules/encyclopedia/*.md` (23 bài EN + VI: WinSxS, Installer, SoftwareDistribution, AppData\Local\Packages, Package Cache, pagefile/swapfile, hiberfil, $Recycle.Bin, System Volume Information, Windows.old, Temp, .ollama, node_modules, Unity Library, Unreal DerivedDataCache, Rust target, Cargo registry, npm cache, NuGet, Gradle, Docker WSL disk, __pycache__, .git): front matter TOML (`paths` biến rule-pack / `names` + `siblings` / `volume_paths`, `risk`, `rules` liên kết rule pack) + mục `## en` / `## vi`; `nfd_rules::encyclopedia` (parser, `validate`, `validate_rule_links`, `Matcher` tra theo đường dẫn → thư mục cha gần nhất), nhúng vào binary (test bắt buộc danh sách khớp thư mục), `nfd rules validate` kiểm cả bách khoa, `nfd explain <path> [--lang vi]`; thẻ "Đây là gì?" trong panel chi tiết (badge mức rủi ro, "Nằm trong: …"). Chỉ là dữ liệu — không thêm hành động xóa nào
- [x] **Age heatmap (A9)** — `insights::age_matrix`: dung lượng theo tuổi sửa đổi (< 1 tháng … > 5 năm, không có ngày) × thư mục con, tab "Tuổi tệp" trong Analyze; **chủ sở hữu** — `nfd_platform::owner` (SID owner → `DOMAIN\user`, fallback chuỗi SID), hiện trong Details ở chế độ Expert
- [x] **Junction / symlink mồ côi (phần còn lại của A3)** — `session::check_links` đọc đích bằng `read_link`, không chạm ổ mạng/ổ quang, mục thứ 3 trong "Dọn gọn" (chỉ xem)
- [x] **G4 "Nếu bấm thì…"** — mọi nút/ô thay đổi trạng thái (quét, quét lại, hủy quét, xuất, Recycle Bin, theme/preset/nhập theme/đặt lại màu, chế độ, ngôn ngữ, cỡ chữ, lịch sử quét, menu Explorer) mang `data-effect` + tooltip giải thích hệ quả; gui-smoke kiểm không nút nào thiếu
- [x] **Tích hợp Explorer (A7 ①)** — `nfd_platform::shell_menu`: "Analyze with NANOFreeDisk" cho thư mục, nền thư mục và ổ đĩa, chỉ HKCU (không cần admin), bật/tắt trong Settings; `nanofreedisk.exe <path>` quét ngay, **single instance** (`tauri-plugin-single-instance` 2.4 — bản cuối hợp MSRV 1.85): lần mở thứ hai chuyển đường dẫn cho cửa sổ đang chạy; gỡ cài đặt xóa khóa (NSIS `NSIS_HOOK_PREUNINSTALL`, WiX `RemoveRegistryKey` removeOnUninstall, hoặc `nanofreedisk.exe --unregister-shell`)
- [x] **Demo video** — `scripts/demo-video.mjs` dựng "ổ demo" giả lập (không lộ dữ liệu cá nhân), chạy exe thật với đường dẫn đó, đi qua các tính năng và ghi bằng CDP `Page.startScreencast`; `scripts/frames-to-video.py` (Pillow) ghép thành `docs/assets/demo/nanofreedisk-demo.gif` / `.webp` (MP4 khi có ffmpeg — máy dev hiện không có ffmpeg nên chưa có MP4); settings và lịch sử của người dùng được khôi phục sau khi quay

**Chuyển sang phase sau (và lý do):**
- Báo cáo **PDF** (A6): giữ là "in báo cáo HTML ra PDF" từ trình duyệt — một engine PDF riêng không đáng thêm dependency.
- Thống kê **dung lượng theo chủ sở hữu** (A9, cả cây): cần đọc security descriptor của từng tệp (chậm trên 1 triệu tệp) → v1.x, cùng MFT reader (Phase 1 *fast mode*) vốn có sẵn owner id.
- Mục "Find duplicates here" / "Compact folder" trong menu Explorer (A7 ①) → Phase 5 (Duplicates) / v1.x (B4 Compact), khi các tính năng đó tồn tại; cột Folder size trong Explorer (A7 ②) giữ ở v1.x.
- Nguồn khác cho Health Score: junk (Phase 3), SMART/NVMe (Phase 7), startup (E2, v1.x).
- Gói MSI/NSIS mang hook gỡ menu Explorer được cấu hình và tauri-build chấp nhận; kiểm tra cài/gỡ thật trên VM thuộc Phase 8 (Installers).

**Kế hoạch ban đầu: lịch sử quét (nền cho A1 và F1)** — đã làm theo kế hoạch này (xem các mục ở trên), khác biệt: lưu thư mục theo ngưỡng kích thước thay vì độ sâu 6, khóa đường dẫn là chuỗi chữ thường (không cần hash), thêm giới hạn 300 snapshot tổng.

1. *Lưu trữ*: crate mới `nfd-history` (SQLite qua `rusqlite` feature `bundled`; kiểm tra MSRV 1.85 và `cargo deny` trước khi thêm). DB ở `%LOCALAPPDATA%\NANOFreeDisk\history.db`. Bảng `scans(id, root, volume_serial, started_at, elapsed_ms, files, dirs, on_disk, free_bytes, total_bytes)`, `dir_sizes(scan_id, path_hash, path, depth, on_disk, files)` — **chỉ thư mục tới độ sâu 6 + mọi thư mục ≥ 50 MB** (không lưu từng tệp: 1 triệu tệp → vài chục nghìn dòng) — và `top_files(scan_id, path, on_disk, mtime)` top 500. Đường dẫn chuẩn hóa chữ thường + `path_hash` (xxh3) để join nhanh. Giữ 30 snapshot / root (cấu hình được), tự xóa cũ; có tùy chọn tắt lưu lịch sử.
2. *A1 What changed?*: `nfd_history::diff(a, b)` → thư mục tăng/giảm nhiều nhất (Δ bytes, Δ files), tệp lớn mới / biến mất, Δ dung lượng trống của ổ (từ `free_bytes` — bắt cả thay đổi ngoài vùng quét). UI: tab "Thay đổi" trong Analyze + chọn 2 snapshot; CLI `nfd diff <root> [--since 7d]`; thêm mục vào báo cáo HTML.
3. *F1 Health Score*: aggregator `HealthContribution { id, score_delta, reason_key, fix_view }` (§13.5). Phase 2 chỉ dùng nguồn đã có: % trống từng ổ, tốc độ đầy (hồi quy tuyến tính trên `free_bytes` lịch sử → F9 "đầy trong ~N ngày"), dung lượng Old & Large / Downloads, số đường dẫn không đọc được; SMART (Phase 7), junk (Phase 3), startup (E2) cộng thêm sau. Mỗi điểm trừ có nút Fix dẫn tới view tương ứng; trang chủ = 1 con số + 3 lý do lớn nhất.
4. *Kiểm thử*: round-trip snapshot, diff trên 2 cây tổng hợp (thêm/xóa/đổi tên thư mục), retention, DB hỏng → tạo lại không crash, ghi < 1 s cho 1 triệu entry.

---

## 13. Mở rộng: từ Disk Analyzer → "Windows Workspace Manager" toàn diện

> Mục tiêu của phần này: liệt kê **mọi tính năng đáng làm** để NANOFreeDisk trở thành công cụ quản lý Windows toàn diện và dễ dùng nhất, có đánh dấu phiên bản dự kiến. Core v1.0 vẫn là 5 trụ cột ở mục 0; các mục dưới đây được xếp vào v1.0 (nhỏ, giá trị cao), v1.x hoặc v2.

### 13.1 Tiêu chí chọn tính năng

1. Liên quan trực tiếp đến **dung lượng, hiệu năng I/O, hoặc sức khỏe máy** — không làm "all-in-one" vô định hướng.
2. Làm được **an toàn** với Undo/Quarantine/Preview; nếu không có cách undo → chỉ *hướng dẫn*, không tự làm.
3. Giải thích được cho người không chuyên bằng **một câu "Nếu bấm thì…"**.
4. Ưu tiên gọi **lệnh chính chủ** của vendor (`rustup toolchain uninstall`, `docker system prune`, `pnpm store prune`, `compact.exe`, `defrag /O`) thay vì xóa thư mục thô.

### 13.2 Danh mục tính năng theo nhóm

Ký hiệu phiên bản: **①** = v1.0 · **②** = v1.x · **③** = v2+. Cột "Rủi ro" là mức cao nhất mà tính năng có thể chạm tới.

#### A. Analyze+ (hiểu ổ đĩa)

| ID | Tính năng | Giá trị cho người dùng | Rủi ro | Ver |
|----|-----------|------------------------|--------|-----|
| A1 | **"What changed?" — so sánh snapshot** giữa 2 lần quét (hôm nay vs tuần trước): thư mục nào phình, file nào mới, bao nhiêu GB | Trả lời câu hỏi phổ biến nhất "sao ổ C tự đầy?" | Đọc | ① |
| A2 | **Old & Large**: file > N GB; file không mở > 1 năm; installer `.exe/.msi/.iso/.zip` cũ trong Downloads; video/ảnh RAW lớn | Dọn nhanh nhất, giá trị lớn nhất | Người dùng tự chọn | ① |
| A3 | **Empty folders, broken shortcuts (.lnk mất target), junction mồ côi** | Gọn máy, bớt lỗi lặt vặt | 🟢 | ① |
| A4 | **Instant search** kiểu Everything trên index đã quét (tên, ext, size, date, attr) + filter lưu được | Tìm "file 4 GB tên gì đó" trong 0,1 s | Đọc | ① |
| A5 | **Folder Encyclopedia**: click bất kỳ thư mục hệ thống/app → "Đây là gì? Xóa được không? Nếu xóa thì sao?" (WinSxS, `AppData\Local\Packages`, `ProgramData\Package Cache`, `Installer`, `.ollama`…), nguồn `rules/encyclopedia/*.md`, cộng đồng đóng góp | Trụ cột "dễ dùng": không ai phải Google nữa | Đọc | ① |
| A6 | **Export báo cáo HTML/PDF** tự chứa, treemap tĩnh, gửi cho IT/sếp | Chia sẻ, lưu bằng chứng | Đọc | ① |
| A7 | **Explorer integration**: context menu "Analyze with NANOFreeDisk", "Find duplicates here", "Compact folder" ①; cột **Folder size** trong Explorer (shell extension COM) ② | Dùng ngay nơi đang đứng | Đọc | ①/② |
| A8 | Quét **profile user khác**, ổ mạng/UNC, VHD/VHDX mount, WSL rootfs (`\\wsl$`) | Admin/IT, dev dùng WSL | Đọc | ② |
| A9 | **Age heatmap & extension sunburst**, "ai sở hữu" (owner SID → tên) | Nhìn là hiểu | Đọc | ① |

#### B. Clean+ & Privacy

| ID | Tính năng | Giá trị | Rủi ro | Ver |
|----|-----------|---------|--------|-----|
| B1 | **Scheduled clean** (Storage Sense nâng cao): chỉ mức 🟢, chạy khi rảnh/pin sạc, báo cáo trước–sau, tắt được từng rule | Tự động mà không sợ | 🟢 | ① |
| B2 | **Recycle Bin manager**: theo ổ, tuổi, kích thước, auto-empty file > 30 ngày, khôi phục | Nhiều người quên Recycle Bin chiếm 50 GB | 🟡 (có xác nhận) | ① |
| B3 | **Profiles**: Developer / Gamer / Office / Minimal → bộ rule + ngưỡng mặc định; chọn lúc onboarding | 60 giây là dùng được | — | ① |
| B4 | **NTFS Compact** (`compact.exe /EXE:LZX` / XPRESS16K) cho thư mục ít thay đổi (game, SDK, `node_modules`, Program Files của app lớn): tiết kiệm 30–60 % **không mất gì**, có đo trước/sau, un-compact được | Giải phóng dung lượng mà không xóa | 🟢 (undo = uncompact) | ② |
| B5 | **Cloud dehydrate**: OneDrive/Google Drive "Free up space" cho file chưa mở N ngày (`FILE_ATTRIBUTE_UNPINNED` / cfapi) | Laptop ổ nhỏ | 🟢 | ② |
| B6 | **Privacy cleaner**: lịch sử/cookie/cache trình duyệt (Chrome/Edge/Firefox/Brave/Arc, với *keep-list* site đăng nhập), Recent items, Jump Lists, DNS cache, clipboard history, Activity history, Windows Search history, thumbnail | Thay CCleaner phần privacy | 🟡 | ② |
| B7 | **Secure delete** (overwrite nhiều lần) cho HDD; trên SSD giải thích TRIM + đề xuất BitLocker thay vì overwrite | Bán/cho máy | 🟠 (xác nhận 2 lớp) | ② |
| B8 | **Downloads Organizer**: tự sắp xếp Downloads theo loại/tháng, gợi ý xóa installer đã cài (so với danh sách Uninstall) | Thư mục bừa nhất của mọi máy | 🟢 (move, undo) | ② |
| B9 | **Hardlink dedup theo chủ đích** cho `node_modules` giữa các project cùng ổ (giống pnpm) | Dev web tiết kiệm hàng chục GB | 🟡 | ③ |

#### C. Dev Workspace (điểm bán hàng với lập trình viên)

| ID | Tính năng | Giá trị | Rủi ro | Ver |
|----|-----------|---------|--------|-----|
| C1 | **DevCache Manager** (mục 4.3) | — | 🟡 | ① |
| C2 | **Toolchain Inventory**: `rustup` toolchains/targets, Node (nvm-windows/fnm/volta), Python (py launcher, pyenv-win, conda envs, venvs), JDK/Gradle/Maven, .NET SDK/runtime, Go, Android SDK (platforms/system-images/NDK), Visual Studio instances & workloads (`vswhere`), Unity Editors (Hub), Unreal Engine versions, Flutter, Docker Desktop. Hiện kích thước, lần dùng cuối, **lệnh gỡ đúng cách** (không xóa thư mục thô) | Máy dev thường có 3–5 phiên bản mỗi thứ không dùng | 🟡 | ① cơ bản / ② đầy đủ |
| C3 | **Repo Finder**: tìm mọi git repo trên ổ; kích thước `.git` vs build artifacts (`target/`, `node_modules/`, `Library/`, `Intermediate/`, `DerivedDataCache/`, `bin/obj`, `.venv`, `__pycache__`, `.next`, `dist`); last commit, stale > N ngày, có thay đổi chưa commit; hành động: xóa artifact (Quarantine), `git gc --aggressive`, archive zip, mở trong IDE | "npkill + cargo-sweep + Unity cleaner" cho mọi ngôn ngữ, 1 màn hình | 🟡 | ① |
| C4 | **Package store cleanup chính chủ**: `npm cache verify`, `pnpm store prune`, `yarn cache clean`, `cargo-cache`, `pip/uv cache`, `nuget locals all -clear`, Gradle/Maven caches, `go clean -modcache`, conda `clean --all`, HuggingFace `hf cache`, Ollama models list/rm | Gọi đúng lệnh → không hỏng store | 🟢/🟡 | ① |
| C5 | **Container & VM**: Docker `system df` + prune theo loại (images dangling/unused, build cache, volumes mồ côi); WSL distro size + **compact `ext4.vhdx`** (hướng dẫn `wsl --shutdown` + `Optimize-VHD`/diskpart); Hyper-V/VirtualBox/VMware disk files & snapshots | Docker/WSL thường chiếm 50–200 GB "vô hình" | 🟡/🟠 | ② |
| C6 | **PATH & Env Doctor**: PATH entry chết/trùng/quá dài (> 2047), tool bị shadow (2 `python.exe`), env var trỏ thư mục không tồn tại; sửa có undo | Hết lỗi "command not found" bí ẩn | 🟡 | ② |
| C7 | **IDE workspace hygiene**: VS Code/Cursor/JetBrains `workspaceStorage` cho project đã xóa, extension không dùng 90 ngày, log cũ | IDE nhanh hơn | 🟢/🟡 | ② |
| C8 | **Log-spam detector** (trong Monitor): tiến trình ghi > X MB/phút liên tục, file log > 1 GB đang mở | Bắt thủ phạm ghi ổ | Đọc | ① |
| C9 | **Build artifact policy**: rule theo marker file (`Cargo.toml`, `package.json`, `.uproject`, `ProjectSettings/`), tùy chọn "giữ artifact repo mở trong 7 ngày gần nhất" | Không xóa nhầm project đang làm | 🟡 | ① |

#### D. Games & Media

| ID | Tính năng | Giá trị | Rủi ro | Ver |
|----|-----------|---------|--------|-----|
| D1 | **Game Library Manager**: Steam (`libraryfolders.vdf`, `appmanifest_*.acf`), Epic, GOG Galaxy, Xbox/Store, Battle.net, Riot, EA, Ubisoft: size, last played, DLC/workshop, shader cache per game, **leftover của game đã gỡ**; di chuyển game giữa library (Steam native / folder move + junction) | Gamer & game dev: ổ 1 TB đầy toàn game | 🟡 | ② |
| D2 | **Capture folders**: NVIDIA ShadowPlay, Xbox Game Bar, OBS, Steam screenshots, Medal — size & dọn theo tuổi | Video 4K chiếm hàng trăm GB | 🟢 | ② |
| D3 | **Shader/GPU cache** (NVIDIA DXCache/NV_Cache, AMD DxCache, Intel, D3DSCache, Steam shadercache, Vulkan pipeline cache) | Có rule sẵn | 🟢 | ① |
| D4 | **Engine cache**: Unity `Library/`, `Temp/`, GI cache toàn cục, Asset Store cache; Unreal DDC toàn cục & per-project, `Intermediate/`, `Saved/Logs`; Godot `.godot/` | Game dev | 🟡 | ① (rule) |
| D5 | **Media duplicates** (perceptual hash ảnh/video gần giống, cặp RAW+JPEG) | Kho ảnh | 🟡 | ② |

#### E. Apps, Startup & "Windows manager"

| ID | Tính năng | Giá trị | Rủi ro | Ver |
|----|-----------|---------|--------|-----|
| E1 | **Uninstaller**: Win32 (registry Uninstall), MSIX/Store (`Get-AppxPackage`), winget-managed; sort theo size/last used/ngày cài; gỡ hàng loạt (chạy uninstaller chính chủ, silent nếu hỗ trợ); **Leftover scan** sau gỡ (AppData/ProgramData/Start Menu/HKCU\Software) → đề xuất Quarantine, không tự xóa | Thay Revo/BCUninstaller | 🟡/🟠 | ② |
| E2 | **Startup Manager**: Run/RunOnce (HKCU/HKLM/WOW6432), Startup folders, Task Scheduler trigger logon/boot, Services auto-start (chỉ non-Microsoft), UWP StartupTask; đo **startup impact** (Diagnostics-Performance Event 100/101, boot trace); **bật/tắt có undo** (dùng `StartupApproved` key như Task Manager), **không bao giờ xóa** | Máy boot nhanh hơn, ít "updater" chạy ngầm | 🟡 | ② |
| E3 | **Scheduled tasks & updaters viewer**: ai chạy lúc nào, bao nhiêu lần/ngày, ghi ổ bao nhiêu (ghép Monitor) | Thấy thứ ẩn | Đọc/🟡 | ② |
| E4 | **App Mover**: chuyển thư mục app/game sang ổ khác + junction; kiểm tra app đang chạy (Restart Manager), dịch vụ, shortcut; undo | SSD nhỏ + HDD lớn | 🟠 | ② |
| E5 | **Storage relocation wizard**: Default save locations (Settings → Storage), Libraries (Documents/Desktop/Downloads/Pictures) sang ổ khác, Temp relocation, pagefile sang ổ khác, hibernation on/off (giải thích Fast Startup), System Restore quota | Toàn bộ "cách Windows dùng ổ" ở 1 nơi, mỗi mục có giải thích | 🟠 (hướng dẫn + xác nhận) | ② |
| E6 | **Context-menu (shell extension) manager**: bật/tắt handler qua `Shell Extensions\Blocked`, không xóa | Explorer nhanh hơn, chuột phải gọn | 🟡 | ③ |
| E7 | **Windows Update & component cleanup có hướng dẫn**: `DISM /StartComponentCleanup`, `/ResetBase` (giải thích mất khả năng gỡ update), `Windows.old` qua Disk Cleanup API, Delivery Optimization cache | Người dùng hay hỏi WinSxS | 🔴→hướng dẫn | ① (hướng dẫn) |
| E8 | **Icon/Font/Search index rebuild** (fix icon trắng, font lỗi, Search chậm) | Sửa lỗi vặt | 🟢 | ② |
| E9 | **Bloatware (Store app) list** với safe-list cộng đồng; gỡ bằng `Remove-AppxPackage` có undo (reinstall từ Store) | Máy mới sạch hơn | 🟡 | ③ |

#### F. Health+, Performance & Monitor+

| ID | Tính năng | Giá trị | Rủi ro | Ver |
|----|-----------|---------|--------|-----|
| F1 | **Health Score (0–100)** trang chủ: free space, SMART/NVMe, nhiệt độ, dirty bit, phân mảnh (HDD), junk size, startup items, backup; mỗi điểm trừ có nút **Fix** dẫn tới module tương ứng | "Dễ dùng" nhất: 1 con số + 3 nút | Đọc | ① |
| F2 | **Disk Benchmark** (Seq 1M Q8T1, Rnd 4K Q32T16/Q1T1 như CrystalDiskMark) + **baseline theo thời gian** của chính ổ đó → phát hiện SSD xuống cấp/throttle | Chẩn đoán "máy chậm" | 🟢 (file test tự xóa) | ② |
| F3 | **Surface scan read-only** (bad sector), **chkdsk scheduler**, **System Integrity wizard** (`sfc /scannow`, `DISM /RestoreHealth`) với giải thích log | Sửa lỗi hệ thống không cần Google | 🟡 | ② |
| F4 | **SSD/HDD optimize**: TRIM (`defrag /O`), kiểm tra TRIM enabled, partition alignment 4K, cluster size, BitLocker, Fast Startup, write-cache policy | Hiệu năng đúng cách | 🟡 | ② |
| F5 | **Backup status**: File History/Windows Backup last run, Macrium/Veeam agent nếu có, nhắc 3-2-1, phát hiện thư mục quan trọng chưa backup | Mất dữ liệu đáng sợ hơn đầy ổ | Đọc | ② |
| F6 | **Resource overview** nhẹ: CPU/RAM/GPU/Net cạnh Disk để tìm thủ phạm "máy lag" (không thay Task Manager) | Một nơi nhìn | Đọc | ② |
| F7 | **"Who touched this?"**: ETW file watcher trên thư mục chọn → process nào tạo/ghi/xóa/đổi tên file, timeline, cảnh báo | Bắt antivirus/indexer/sync/hành vi giống ransomware | Đọc | ② |
| F8 | **Folder quota & alert**: cảnh báo khi Downloads > 20 GB, `.ollama` > 50 GB, ổ C < 10 % | Không bao giờ bất ngờ | Đọc | ① |
| F9 | **Free-space forecast** ("ổ C đầy trong ~23 ngày") ①; **Boot time trend** (Event 100) ③ | Dự báo | Đọc | ①/③ |
| F10 | **Memory dump / event log / WER** size & policy advisor | Máy dev hay có dump 8 GB | 🟡 | ① (rule) |

#### G. Trải nghiệm "dễ dùng nhất"

| ID | Tính năng | Ver |
|----|-----------|-----|
| G1 | **Trang chủ = Health Score + 3 nút lớn**: "Giải phóng *X* GB an toàn ngay", "Xem thứ gì chiếm ổ", "Ổ đĩa có khỏe không" | ① |
| G2 | **Simple / Expert mode**: Simple ẩn Advanced rules, thuật ngữ, raw SMART; Expert hiện tất cả + CLI hint cho mỗi hành động ("lệnh tương đương: `nfd clean --rules ai-tools`") | ① |
| G3 | **Command palette (Ctrl+K)** + keyboard-first + search mọi thứ (ổ, thư mục, rule, setting) | ① |
| G4 | **Tooltip "Nếu bấm thì…"** trên mọi hành động phá hủy; preview danh sách file trước khi làm; số GB sẽ giải phóng | ① |
| G5 | **Tray**: free space mỗi ổ, cảnh báo, quick clean 🟢, "pause monitor" | ① |
| G6 | **Time Machine / Activity log**: mọi việc app đã làm theo ngày, Restore 1 click (Recycle Bin/Quarantine/Startup toggle/Compact) | ① |
| G7 | **Onboarding 60 giây**: chọn profile → chọn ổ → quét → kết quả; không cần đọc docs | ① |
| G8 | **Read-only / Demo mode** cho IT khi hỗ trợ máy người khác; **Policy file** (`policy.toml`) để admin khóa tính năng | ② |
| G9 | **i18n EN/VI bắt buộc** (fluent, CI kiểm tra key đủ 2 ngôn ngữ, cả rule `impact`) · **Theme Light / Dark / System / Custom** (người dùng tự đặt màu, preset, xuất/nhập `theme.toml`, design tokens) · font scale, high-contrast, screen-reader labels — chi tiết mục 11 | ① |
| G10 | **Report & automation**: HTML report ①; CLI JSON ①; **PowerShell module** `NANOFreeDisk` wrapper ②; webhook/Task Scheduler integration ② | ①/② |
| G11 | **Portable, offline, no telemetry**; update check tắt được | ① |
| G12 | **Community rule packs & encyclopedia**: `nfd rules add <url>` có chữ ký minisign, review qua PR, hiển thị "verified by" | ③ |
| G13 | **Compare 2 máy / 2 snapshot** export–import (`.nfdreport`) để IT so sánh | ③ |

### 13.3 Cố ý KHÔNG làm (và lý do)

| Không làm | Lý do |
|-----------|-------|
| Registry cleaner | Gần như vô ích trên Windows hiện đại, rủi ro cao, không undo chắc chắn |
| Driver updater | Nguồn driver không tin cậy, dễ gây BSOD; để Windows Update/vendor |
| "RAM optimizer", "Game booster", "Internet booster" | Placebo, làm giảm uy tín sản phẩm |
| Antivirus / malware removal | Ngoài chuyên môn; chỉ **cảnh báo hành vi** (F7) và gợi ý Defender scan |
| Tự kill process / dừng service | Chỉ hiển thị và để người dùng tự làm |
| Chỉnh telemetry/privacy sâu Windows, gỡ Windows component | Để tool chuyên (O&O ShutUp, DISM); app chỉ hướng dẫn |
| Kernel driver / minifilter | Chi phí ký driver, rủi ro ổn định; ETW đã đủ |

### 13.4 Ảnh hưởng tới lộ trình

- **v1.0 (Windows)** giữ 5 trụ cột + các mục ① ở trên; ước tính **+3 tuần** (A1–A6, A9, B1–B3, C2 cơ bản, C3, C4, C9, D3, D4, E7 hướng dẫn, F1, F8, F9 forecast, F10, G1–G7, G9–G11). Lộ trình mới **~21 tuần**: Phase 2 thêm 1 tuần (A1–A6, A9, G1–G4, F1), Phase 4 kéo dài thành 3 tuần (thêm C2/C3/C4/C9), các mục ① còn lại rải vào phase tương ứng.
- **v1.1 – v1.3**: B4 Compact, B6 Privacy, C5 Container/VM, C6 PATH Doctor, C7, D1 Games, D2, E1 Uninstaller, E2 Startup, E3, E4/E5 Mover & Relocation, E8, F2 Benchmark, F3–F7, G8, G10 PowerShell. Mỗi bản minor chọn 2–3 module, release mỗi 4–6 tuần.
- **v2.0**: cross-platform (mục 8, Phase 10) + B9, D5, E6, E9, F9 boot, G12, G13.

### 13.5 Kiến trúc cho mở rộng

- Mỗi nhóm A–G là một **feature crate** riêng (`nfd-apps`, `nfd-startup`, `nfd-games`, `nfd-devtools`, `nfd-privacy`, `nfd-bench`) phụ thuộc `nfd-core` + `nfd-platform`, bật qua Cargo feature flag → binary nhỏ, test độc lập, port từng phần sang macOS/Linux.
- Mọi hành động phá hủy mới **bắt buộc** đi qua `Planner → SafetyGate → Executor → UndoJournal` (mục 3.4); mỗi module khai báo `UndoStrategy` (RecycleBin / Quarantine / RegistryBackup / Uncompact / ReinstallHint). Module không có undo → chỉ được ở chế độ *Guide* (hiển thị lệnh, không thực thi).
- UI: mỗi module là 1 route Svelte + 1 lệnh CLI tương đương; Health Score (F1) là aggregator đọc `HealthContribution` từ mọi module.
- Encyclopedia (A5) và rule packs dùng chung schema `rules/` để cộng đồng đóng góp bằng PR markdown/TOML, có CI validate.
