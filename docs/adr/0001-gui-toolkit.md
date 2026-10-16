# ADR 0001 — GUI toolkit: Tauri 2 + Svelte 5 (egui stays the installer toolkit)

- **Status:** accepted — 2026-10-16 (Phase 2 checkpoint, PLAN §2 / §8)
- **Deciders:** Lưu Đức Thành Công (NANO)

## Context

PLAN §2 picked Tauri 2 + Svelte 5 for the main app with egui as "Option B", to be
confirmed at the end of Phase 2 after a treemap prototype. The worry was that a
TypeScript front end would slow a solo developer down or that a web canvas would not
reach the Phase 2 target: **a 50k-rectangle treemap at 60 fps**.

## What was built and measured

The first Phase 2 slice — drive overview, scan with live progress, treemap, sunburst,
virtualized tree list, search & filters, details, Recycle Bin with safety checks,
EN/VI, Light/Dark/System/Custom themes — was written in Tauri 2.11 + Svelte 5.

Treemap benchmark (squarified layout + canvas 2D, Chromium/WebView2, 1600 × 1000 px,
50 201 nodes, 48 576 visible tiles; medians of 10 runs on the development machine):

| Step | Time |
|---|---|
| Layout (`src/lib/treemap.ts`) | ~2.3 ms |
| Paint, naive (`fillStyle` per tile) | ~18.7 ms |
| Paint, bucketed by color (shipped) | ~15 ms |
| Hit test (pointer → tile) | ~0.07 ms |

A full repaint (resize, zoom) is ~17 ms; hovering and selection only redraw a
transparent overlay canvas (< 1 ms). Typical views are much lighter because tiles
under 5 px² are folded into a "small items" tile per folder (the 80k-entry mock
drive shows ~4.4k tiles in a 580 × 390 px treemap).

The Rust side stayed UI-agnostic: `nfd-core` gained `query` (search/filters) and
`ScanTree::remove`, `nfd-platform` gained `volumes`, `trash` and `appearance`; the
Tauri crate is ~1.1k lines (thin command wrappers plus `session.rs`, which is unit
tested without a window).

## Decision

Keep **Tauri 2 + Svelte 5 + TypeScript** for the main application.

- The 60 fps target is met with plain canvas 2D; WebGL is not needed.
- Svelte made the theme/i18n requirements (live switching, CSS-variable tokens,
  Fluent) cheap; doing the same in egui `Visuals` is possible but slower to iterate.
- The bundler gives MSI + NSIS (+ DMG/AppImage later) for free (PLAN §6).
- A browser-only mock backend (`src/lib/mock.ts`) lets the UI be developed and
  screenshotted without a native build.

**egui** remains the toolkit for the branded installer (`apps/nanofreedisk-setup`),
which must not depend on WebView2 being present.

## Consequences

- MSRV 1.85 caps Tauri at 2.11.x (2.12 needs Rust 1.90). The npm packages are pinned
  to the matching minor (`@tauri-apps/api` 2.11, `plugin-dialog` 2.7) — `tauri build`
  refuses mismatched majors/minors. Revisit when the MSRV is raised.
- `cargo-deny` bans `winapi` for our code but allows it through `dlopen2` and
  `gio-sys`, which Tauri's runtime pulls in.
- Linux CI jobs need the WebKitGTK development packages to compile the app crate.
- The frontend has its own quality gates: `svelte-check --fail-on-warnings`, unit
  tests (`node --test`), and `scripts/check-locales.mjs` for EN/VI parity.
