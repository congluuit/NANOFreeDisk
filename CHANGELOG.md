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
- `nfd diff <root> [--since 7d] [--json]` compares saved scans from the command line (`nfd scan --save` saves one; the app's history is shared), and the HTML report gains a "What changed since…" section.
- **Folder Encyclopedia**: 23 English/Vietnamese articles in `rules/encyclopedia/*.md` (WinSxS, Windows Installer, pagefile, `$Recycle.Bin`, AppData\Local\Packages, .ollama, node_modules, Unity Library, Rust target, .git…) explaining what a folder is, what happens if it is deleted and what to do instead. Shown as "What is this?" in the Details panel and by `nfd explain <path>`; validated by `nfd rules validate`. Information only — no delete action is attached.
- **Age heatmap** tab in Analyze (space by last-modified age per sub-folder), the **owner** of the selected item (Expert mode), and **orphan junctions / broken symbolic links** in Tidy up (read-only).
- "If you do this…" tooltips on every control that changes something.
- **Explorer integration**: optional "Analyze with NANOFreeDisk" in the right-click menu of folders and drives (per user, no admin rights; removed when turned off or on uninstall), `nanofreedisk.exe <folder>` starts a scan, and a second launch hands its folder to the window already open.
- Demo recording of the real app (`scripts/demo-video.mjs`, `docs/assets/demo/`).

## [0.1.0] - unreleased

Reserved for the first tagged pre-release (core scanner + CLI).

[Unreleased]: https://github.com/congluuit/NANOFreeDisk/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/congluuit/NANOFreeDisk/releases/tag/v0.1.0
