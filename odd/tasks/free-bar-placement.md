# Feature: free-bar-placement — the bar stays where you drop it, and one instance only

Status: **implemented, manual checks pending** (opened 2026-09-29). Branch `fix/free-placement` (from `dev`, `b98d463`).

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
- [x] **F3 — Frontend uses free placement.** Menu, panel and collapse use F2 instead of
      re-snapping to the right edge; the menu/panel render on the side F2 returns; drag
      release persists position without snapping; startup restores the saved position.
      Checks: CI set; manual — bar away from the edge, open menu from right-click and from
      the settings cell, open panel, collapse/expand, near left and right edges, restart.
- [x] **F4 — Docs.** README (what you get, using, config fields), `docs/architecture.md`,
      `AGENTS.md` if a rule changed.
- [x] **F5 — Review fixes.** Serialize every placement; convert physical offsets to CSS px.
      Checks: CI set; manual — quick menu/collapse/drag sequences, scale factor above 100%.

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
- F2 done, commit `4c61371` (TDD on, runner `cargo test`). RED: `cargo test` with `unimplemented!()` stubs and 20 new
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
- F3 done, commit `88ed6bb`. `main.ts`: `applyState` calls `place_window`, reading the bar back from the live window
  (position + last placement offset) so a drag is never undone; drag release re-places (clamps)
  and persists position + monitor only if the bar moved; collapse/expand persists the re-anchored
  position; startup is restored in Rust (`restore_bar`). `styles.css`: `body.side-right` flips
  the panel/menu; collapsed tab offset via `--ob-bar-dy`. `snap_window` removed.
  `npx tsc --noEmit` clean; `npm run build` ok; `cargo test` 135 passed; clippy clean.
  Manual, pending (cannot run GUI here): bar away from the edge, menu from right-click and from
  the settings cell, panel, collapse/expand, near left and right edges and top/bottom, drag then
  restart, second launch. No route deviation: one writer, no delegation.
- F4 done. README (intro, What you get, Using, config table incl. `position`), `docs/architecture.md` (placement, single instance, commands table; removed a stale `body.resizing` claim), `app/README.md` layout. `AGENTS.md` unchanged: no rule changed. Checks re-run: tsc clean, build ok, `cargo test` 135 passed, clippy clean.

- Review (RDD, slice `b98d463..8d6dc8e`, 1224 lines, medium, user granted): **approved**,
  acknowledged. Non-blocking follow-ups: (W) `applyState` bar reconstruction, drag-persist
  decision and `rememberBar` are untested frontend state — extract to pure functions when a
  frontend runner exists; (S) no test for `place_window` monitor fallback (`lib.rs:186-193`);
  (S) single-instance registration gated on `cfg(desktop)` but the dependency only on
  windows/macos/linux (`lib.rs:290-291` vs `Cargo.toml`) — align the conditions.
  Writer open question: should "Reload config" also apply an edited `position`? (startup-only now).
  Known cosmetic: chevron does not flip in the left half; possible one-frame jump collapsing a
  left-half bar.
- F5 done (review WARNINGs `R3-concurrent-applystate-stale-offset`, `R3-bar-dy-physical-as-css`;
  user approved fixing them before the manual GUI test). `main.ts`: every `place_window` call now
  runs through one promise queue (`enqueuePlacement`; requests apply in order, none dropped);
  position + offset are read inside the critical section; the drag-release and collapse/expand
  config writes (`applyAndRemember`) happen inside it too. `--ob-bar-dy` and the menu's height
  clamp use physical -> CSS px via `win.scaleFactor()` (also the collapsed-tab paint guess).
  New DOM-free `placement-view.ts` (`physicalToCss`, `barFromWindow`, `requestKey`, `barMoved`,
  `collapsedTabDyCss`), untested until a frontend runner exists. The fixed 72x400 / 46x46 window
  geometry still assumes physical == CSS pixels (pre-existing; at scale > 100% the bar itself is
  still mis-sized, not addressed here). `npx tsc --noEmit` clean; `npm run build` ok;
  `cargo test` 135 passed; clippy clean. Commit: see git log
  (`fix: serialize bar placement and convert offsets to CSS pixels`).

## Next step

User: manual GUI checks (see F3; also try a scale factor above 100% and quick menu/collapse/drag
sequences), then PR `fix/free-placement` to `dev`.
