# Feature: free-bar-placement — the bar stays where you drop it, and one instance only

Status: **in progress** (opened 2026-09-29). Branch `fix/free-placement` (from `dev`, `b98d463`).

## Objective

Let the bar live anywhere on screen: it stays where the user drops it, and the context menu
and usage panel open next to it on whichever side has room. Also allow only one running
Orbitbar at a time.

## Problem / why

Reported by the user on 2026-09-29 against `v0.1.0` (portable `target/release/orbitbar.exe`):

- **Menu breaks when the bar is not at the right edge.** With the bar a little away from the
  right edge, picking any menu entry snaps the bar to the right edge and breaks the menu. From
  the settings cell the menu opens on top of the bar, covering the other entries, and is cut
  off. The design assumes the bar always sits on the right edge: the menu and the panel widen
  the window leftwards and re-snap it to the right edge (`sizeFor` / `applyState` /
  `snapToMonitor`, `app/src/main.ts:119`, `:274`, `:238`). When the bar is not there, the
  re-snap moves it and the menu is drawn on a window that just changed size and place.
  Likely also the cause of the visible flash when opening menu entries.
- **More than one bar.** Launching `orbitbar.exe` again starts another bar. There is no
  single-instance guard (no `tauri-plugin-single-instance`).

## Decisions

- User (2026-09-29): **option B, free placement**, over "always snapped to the right edge".
- Assumed defaults (agent, recorded here, easy to revisit):
  - The bar position persists in `config.json` and is restored on start. A saved position
    that is off every monitor falls back to today's right-center placement.
  - Opening or closing the menu or panel never moves the bar on screen. The window grows
    toward the side with room (left first, as today; right when there is no room on the
    left), and the menu/panel render on that side of the bar.
  - Nothing may be drawn off the monitor: the grown window is clamped to the monitor's work
    area, vertically too.
  - Dragging no longer snaps to the right edge; the release only clamps into the monitor and
    persists the position and monitor.
  - A second launch exits and brings the running bar forward.

## Scope

In: window placement for bar, collapsed tab, context menu and usage panel; drag release;
position persistence; single instance; docs that describe docking. Out: new settings UI,
multi-bar support, installer changes.

## TDD

Mode: **on** (session config "Strict TDD Mode: enabled"). Runner: `cargo test` in
`app/src-tauri` (there is no frontend test runner). Placement math lives in pure Rust so it
is test-first; the frontend only applies the result.

## Tasks

- [x] **F1 — Single instance.** Register `tauri-plugin-single-instance` first in the
      builder; a second launch exits and shows/unminimizes/focuses the existing window.
      Checks: CI set; manual — launch the release exe twice, one bar.
- [x] **F2 — Pure placement logic (Rust, test-first).** Given the bar's rect, the extra
      size the menu/panel needs, and the monitor work area, return the window rect and the
      side (left/right) the extra content goes on, keeping the bar's on-screen position
      fixed and everything inside the work area. Also the restore rule for a saved position
      (valid → use it, clamped; off-screen or absent → right-center). Exposed as Tauri
      commands. Checks: RED then GREEN `cargo test`, clippy.
- [ ] **F3 — Frontend uses free placement.** Menu, panel and collapse use F2 instead of
      re-snapping to the right edge; the menu/panel render on the side F2 returns; drag
      release persists position without snapping; startup restores the saved position.
      Checks: CI set; manual — bar away from the edge, open menu from right-click and from
      the settings cell, open panel, collapse/expand, near left and right edges, restart.
- [ ] **F4 — Docs.** README (what you get, using, config fields), `docs/architecture.md`,
      `AGENTS.md` if a rule changed.

Route: one delegated writer for F1–F4 (2+ non-trivial files: `lib.rs`, `config.rs`,
`main.ts`, `styles.css`, `Cargo.toml`, docs). One work-unit commit per task.

## Checks (CI set)

```sh
cd app && npx tsc --noEmit && npm run build
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
```

## Delivery

Strategy: `ask-on-risk`. Forecast ~350–500 authored lines; PR `fix/free-placement` → `dev`,
then promote to `main` without a tag unless the user asks for a release.

## Progress / evidence

- 2026-09-29: explored; causes confirmed in code (see Problem).
- F1 done (route: delegated writer). `tauri-plugin-single-instance` 2.5.0 registered first
  (desktop only). `npx tsc --noEmit`: clean; `npm run build`: ok; `cargo test`: 113 passed;
  `cargo clippy --all-targets -- -D warnings`: clean. Commit `17f834e`.
  Manual, pending: launch the release exe twice, one bar, the second focuses the first.
- F2 done (TDD on, runner `cargo test`). RED: `cargo test` with `unimplemented!()` stubs and 20 new
  tests: `test result: FAILED. 113 passed; 20 failed` (all 20 `placement::tests::*`, panic
  `not implemented`); config tests failed to compile (`no field position`, `Position` missing).
  GREEN: `135 passed; 0 failed; 1 ignored`. `cargo clippy --all-targets -- -D warnings`: clean;
  `npx tsc --noEmit` clean; `npm run build` ok.
  Design: new `placement.rs` (pure `clamp_into`, `pick_area`, `restore_bar`, `anchor_resize`,
  `place`); command `place_window({bar, barWidth, barHeight, extraWidth, extraHeight})` returns
  `{bar, window, side, barOffset, monitor}`; uses `Monitor::work_area()` (taskbar excluded).
  Config: optional `position: {x, y}` (skipped when unset). Collapse/expand anchoring: vertical
  center kept, horizontal edge nearest the monitor edge kept. `snap_window` is kept only until F3
  switches the frontend, so this commit still runs.

## Next step

F3.
