# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/).

Entries are generated from Conventional Commits with `git-cliff`; edit by hand only to add context.

## [Unreleased]

### Added
- Project foundation: Cargo workspace (`nfd-core`, `nfd-rules`, `nfd-platform`, `nfd-cli`), CI, release pipeline skeleton.
- DevCache catalog v0 (`docs/DEVTOOL_CACHE_CATALOG.md`) and the `ai-tools` / `windows-system` rule packs.
- Rule-pack loader and validator (`nfd rules validate`).
- Development plan (`docs/PLAN.md`) including the §13 feature-expansion map (v1.0 / v1.x / v2 tags and the "never do" list), mirrored in the README roadmap.
- Scanner, standard mode (`nfd_core::scanner`): parallel directory walk that reads metadata straight from NTFS directory listings (~1M entries in under 4 s on an SSD), never follows junctions, symlinks or mount points, counts hard-linked data once, and collects unreadable paths as issues instead of failing. Progress streams over a `ScanEvent` channel and scans can be cancelled.
- Directory size roll-up, largest files and per-extension breakdown.
- `nfd scan <path>` with `--top`, `--json` (schema `nanofreedisk.scan.v1`, see `docs/schema/scan-v1.schema.json`), `--csv`, `--min-size`, `--threads`, `--exact-links` and a progress spinner. `--fast` is accepted but falls back to the standard scan until the NTFS MFT reader lands (cargo feature `mft`).
- `nfd_platform::fsinfo`: Windows file metadata (allocation size, compression-aware size, reparse tag, file ID, link count) via `windows-sys`.
- Desktop app (`apps/nanofreedisk`, Tauri 2 + Svelte 5): drive overview, background scan with live progress and cancel, treemap and sunburst, virtualized folder tree, largest files, file types, search & filters (name/glob, size, extension, date, hidden/system, cloud-only), details panel, Show in Explorer / Open / Copy path, JSON/CSV export.
- Move to Recycle Bin from Analyze, behind `nfd_core::safety::check_manual_delete`: volume roots, the Windows folder, Program Files, credentials and system folders on every drive are refused; user/AppData/Temp folders themselves are refused while items inside them can be picked; links, junctions and folders containing mount points are refused; every item is re-checked on disk right before the move, large deletions need an extra confirmation, and Windows asks before anything too big for the bin would be deleted permanently (`nfd_platform::trash`).
- `nfd_core::query` (search & filters over a scan) and `ScanTree::remove` (keep totals right after deletions); `nfd_platform::volumes` and `nfd_platform::appearance` (Windows accent color).
- English and Vietnamese UI via Fluent (`locales/`), with `scripts/check-locales.mjs` keeping both complete; Light / Dark / System / Custom themes with presets, `theme.toml` import/export and WCAG contrast warnings; adjustable text size.
- App icon and brand guide (`docs/BRAND.md`); GUI toolkit decision record (`docs/adr/0001-gui-toolkit.md`).
- **Old & large** view (`nfd_core::insights::old_and_large`): files above a size you choose, big files not modified for a year or more, and old installers / disk images left in any `Downloads` folder; pick several and move them to the Recycle Bin through the same safety checks as Analyze.
- **Tidy up** view (read-only): outermost empty folder trees (`insights::empty_folders`; unreadable folders are never called empty, and folders the safety rules protect are skipped while empty folders inside them are listed) and shortcuts whose target is gone (`nfd_core::shortcut`, a Shell Link reader that never resolves through the shell or touches network drives; "target missing" vs "drive not connected").
- Self-contained **HTML report** (static SVG treemap, largest items and files, file types, Old & Large summary, current theme colors, EN/VI) from Analyze → Export.
- **Simple / Expert** mode: Simple hides advanced filters, attributes, unreadable paths and JSON/CSV export; Expert shows everything plus the equivalent `nfd` command with a copy button.
- **Command palette (Ctrl+K)**: views, actions, drives, folders of the current scan, language / theme / text size; matching ignores case and Vietnamese diacritics.
- `scripts/check-locales.mjs` also fails when the UI uses a message key missing from `locales/en/app.ftl`; `scripts/gui-smoke.mjs` covers the new commands and views.
- **Scan history** (`nfd-history`, SQLite compiled in): every scan can be saved as a compact snapshot (folder sizes from 1 MB, large files) on this PC only; 30 snapshots per folder, 300 in all; a damaged database is set aside and replaced. Turn it off or clear it in Settings.
- **What changed?** tab in Analyze: compares the scan with an earlier one of the same folder — folders that grew or shrank (shown at the most specific level), large files added or removed, and the change in the drive's free space; click a row to jump to it.
- **Health score** on Home (`nfd_core::health`): 0–100 from free space per drive, how fast each drive fills (from the scan history) and the Old & Large total, with the three biggest reasons and a button to fix each.
- **Hardware view** (docs/HARDWARE_MONITOR.md, read-only, no driver, no admin):
  - **Overview** (`nfd-hwinfo`, schema `nanofreedisk.hwinfo.v1`): system and Windows build, CPU from CPUID with the hybrid P/E split, caches and clocks, mainboard / BIOS / memory sticks from SMBIOS, GPUs, disks (bus, SSD/HDD, temperature), monitors, network adapters, battery wear and cycles; copy as text in the UI language, JSON export (Expert). Serial numbers, UUIDs, MAC and IP addresses are never collected.
  - **Sensors** (`nfd-sensors`): Value / Min / Max tree like LibreHardwareMonitor with a one-minute sparkline — CPU load per thread and estimated clock, memory, GPU load per engine / VRAM / temperature / clocks / power (D3DKMT, PDH, NVML), disk temperature and activity, battery, network, ACPI zones. A sleeping discrete GPU is never woken up. Sampling runs only while the tab or the Home card is shown. CPU temperatures, power and voltages say they need Advanced sensors (PawnIO, opt-in, coming with the elevated helper).
  - **Home temperature card**: CPU / GPU / hottest disk, opens Sensors.
  - **Tasks** (`nfd-tasks`): Task Manager-like list with CPU, private memory, disk I/O and GPU per process, publisher (Authenticode), Apps / Background / Windows groups, tree view, search and sort. Close app / End process / End process tree only when the user picks it and confirms; the process is re-identified by PID and creation time, critical / protected / core Windows processes and NANOFreeDisk itself are hard-blocked, svchost (with its services), Explorer and programs in the Windows folder need a second confirmation; every attempt is logged to `logs/tasks.log`.
  - CLI: `nfd hw [--json]`, `nfd sensors [--watch] [--simple] [--json]`, `nfd ps [--sort cpu|memory|io|gpu|name|pid] [--json]` (read-only).
- **Advanced sensors (PawnIO, opt-in, ADR 0002)**: an elevated helper (`nanofreedisk.exe --elevated-helper`, `nfd-helper`) started through UAC on request, talking to the app over a local named pipe with a three-request protocol (discover / sample / shutdown) and PID + executable checks on both ends. Through PawnIO's official IntelMSR module (SHA-256 pinned, IOCTL interface only, `ioctl_read_msr` only) it adds CPU package and per-core temperatures, core max / average, TjMax, RAPL package / cores / graphics / platform power, PL1 / PL2, per-core VID, per-core and effective clocks. The Sensors tab shows PawnIO's state (not installed / not running / module missing) and a button to turn them on; `nfd sensors --advanced` does the same from an elevated prompt. Nothing is ever written to the hardware.
- **AMD Zen Advanced sensors** (PawnIO AMDFamily17, SHA-256 pinned, read functions only): Tctl / Tdie, per-CCD temperatures, package and per-core power, per-core VID, effective clocks. Not yet verified on AMD hardware. Mainboard Super I/O (fans, board voltages) is deliberately not probed until a table of chips verified on real boards exists: entering a chip's configuration mode writes to ports 0x2E / 0x4E, which on laptops often belong to the embedded controller.
- **Temperature alerts** (Hardware → Sensors, off by default): a Windows notification and an in-app toast when the CPU reaches 95 °C, a GPU 90 °C or a drive its own warning temperature (NVMe WCTEMP; 70 °C when it reports none), once per episode with a 15-minute cooldown. While on, sensors are read every 5 s in the background; otherwise only while a view asks.
- **Last 24 hours** chart of the hottest CPU / GPU / drive per minute, kept in `thermal-24h.json`, and a **Health Score** contribution for parts that ran hot (−8) or warm (−2) in that window, with a "See temperatures" fix button.
- Drives' own warning / critical temperatures shown in the Sensors tree and used for colors, alerts and the Health Score.
- Validation against LibreHardwareMonitor on the reference i9-13900HX (`docs/hwmon-validation.md`, `scripts/hw-compare-lhm.mjs`); the bus clock is now TSC-derived and core registers are read in two passes, as LHM does.
- The i18n loader and `scripts/check-locales.mjs` read every `locales/<lang>/*.ftl` (one file per feature area).

## [0.1.0] - unreleased

Reserved for the first tagged pre-release (core scanner + CLI).

[Unreleased]: https://github.com/congluuit/NANOFreeDisk/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/congluuit/NANOFreeDisk/releases/tag/v0.1.0
