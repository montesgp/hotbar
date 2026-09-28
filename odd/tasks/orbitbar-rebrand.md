# Feature: orbitbar-rebrand — standalone, cross-platform, no personal data in the repo

Status: **in progress** (opened 2026-09-28). Branch `feat/orbitbar` (from `feat/autostart`, `46d30bc`).

## Objective

Turn the personal "hotbar" into **orbitbar**: an open-source, cross-platform bar any developer
can install. It mounts on the OS and, as base metrics, shows token spend and related info for
the AI agents installed locally (claude, codex, opencode), per project and per time window.

## Problem / why

- The product is still named hotbar/herdr-omniroute and carries Herdr/OmniRoute pieces.
- Personal data is committed: absolute user paths, machine name, a key-like string.
- The daily driver (legacy PowerShell/WPF widget in `hotbar/`) reads usage from the agents'
  own files, but it also reads `%APPDATA%\herdr\session.json` (`Get-HerdrOpenProjects`,
  `hotbar/lib/Get-AgentUsage.ps1:882-922`) to highlight projects open in Herdr. That is a
  dependency on an external program.
- The Tauri app (the product) has no usage readers yet: `agent-usage` is a stub
  (`hotbar-tauri/src/main.ts:324`). Replacing the legacy widget today would lose the metrics.

## Constraints (user decisions, 2026-09-28)

- Orbitbar depends on no external service or program. Herdr/OmniRoute are optional extensions.
- Personal settings live in local, gitignored files (`.env` or local config). The public repo
  ships only `*.example` files that reproduce the author's setup without secrets.
- Every remaining "hotbar" becomes "orbitbar".
- The user's environment must keep working as today (token spend per project over a time
  window) throughout the migration.
- Autostart on by default, configurable off (done in HB29, `a80178a`).

## Tasks

- [x] **O1 — Personal data out of the repo.** Root `.gitignore` (`.env`, local configs);
      `hotbar/config.json` untracked and replaced by `config.example.json` (legacy widget
      falls back to the example when the local file is missing); key-like string, machine
      name and absolute user paths removed from tracked files (paths become env vars or
      PATH lookups); `.env.example` for the extension settings. Local files keep working.
      Route: delegated writer (6+ files). Evidence: PS parse 0 errors (`hotbar.ps1`,
      `start.ps1`); `git grep --cached` for user/machine/key → no hits; `.env` and
      `hotbar/config.json` ignored. Not typechecked: `extensions/omniroute.ts` has no tsc
      scope in this repo. **Pending (user):** create the local `.env` with the real
      `OMNIROUTE_NODE` / `OMNIROUTE_ENTRY` (tool guardrail blocks writing `.env`); until
      then the OmniRoute start actions fall back to `node` on PATH.
- [x] **O2 — Drop the Herdr dependency from the usage reader.** Remove
      `Get-HerdrOpenProjects` and its consumers in `hotbar.ps1`; projects come only from the
      agents' session `cwd`s. Usage panel keeps working. Herdr's session list only chose
      which repo roots to show; the panel now lists each agent's top 5 projects of the month
      by output tokens (`$script:AgentHistoryMaxProjectsShown`), one row per session `cwd`
      (no merging of subfolders into a repo root).
- [x] **O3 — Move Herdr/OmniRoute out of the core.** `herdr-plugin.toml`, `extensions/`,
      `scripts/`, OmniRoute readers and the `omniroute-status` action go to
      `extensions/herdr/` and `extensions/omniroute/`, clearly marked optional. OmniRoute
      readers load only when present; missing extension renders "extension not installed".
      Omniroute item removed from default items (Tauri + example config). `.env` loaders
      strip quotes; unset `OMNIROUTE_ENTRY` fails with a clear message.
      Route (O2+O3): one delegated writer (shared `hotbar.ps1`). Evidence: PS parse 0 errors
      on 13 files; `Get-AgentHistorySnapshot` without Herdr → claude/codex/opencode `Ok`;
      `hotbar.ps1 -SelfTest` → PASS on the author's machine (gateway UP, 5 panel lines);
      `cargo test` 13/13. O1 independent verifier: PASS (two minor findings fixed here).
      RDD: two candidates declined by the user (base `main` and base `origin/main`).
      **Follow-up:** `.env.example` sets `OMNIROUTE_ENTRY=omniroute/bin/omniroute.mjs` as a
      value; it should be a commented placeholder (tools cannot edit `.env*` files).
- [x] **O4 — Rename hotbar → orbitbar.** Folders, Tauri identifier/productName, Cargo and npm
      package names, UI copy, config paths. Identifier change moves the config dir
      (`com.hotbar.app` → `com.orbitbar.app`): migrate the existing config on first launch.
      Layout: `hotbar-tauri/` → `app/`, `hotbar/` → `legacy/windows-widget/`
      (`orbitbar.ps1`, `launch-orbitbar.ps1`; local `config.json` moved on disk),
      `extensions/omniroute/legacy-widget/`. Crate `orbitbar`/`orbitbar_lib`, npm `orbitbar`,
      mutex `Local\orbitbar.widget.v1`, CSS `--ob-*`, default tooltips in English.
      `migrate_legacy_config` copies (never moves) the old config. Route: delegated writer.
      Evidence: RED (`migrate_legacy_config` not found) → GREEN; `cargo test` 16/16 (parent
      re-ran: 16/16); `cargo build` OK; `tsc --noEmit` clean; PS parse 0 errors (8 files);
      `orbitbar.ps1 -SelfTest` PASS. Outside docs (`docs/`, `README.md`, `app/README.md`,
      left for O6 on purpose) only `com.hotbar.app` remains, in migration code.
      O4 independent verifier: code PASS; flagged `app/README.md` still says hotbar and
      `--hb-*` → carried into O6. O2+O3 independent verifier: PASS, no defects. RDD: 4th candidate (base `origin/main`)
      declined. **Side effects:** the writer stopped two running `hotbar-tauri.exe`
      processes to unlock the folder; a stale `HKCU\...\Run\Hotbar` entry points to the old
      `hotbar-tauri\...\release\hotbar-tauri.exe`.
- [x] **O5a — Rust readers + `get_usage(window)` command** (`app/src-tauri/src/usage/`:
      claude, codex, opencode via bundled rusqlite, pricing; `TimeWindow` today | last7Days |
      last30Days | thisMonth; per-agent status ok | notInstalled | error). Route: delegated
      writer. Evidence: RED → GREEN, `cargo test` 49/49 (+1 ignored real-data parity test;
      parent re-ran 49/49), clippy clean, build OK. Real-data scan ~2.7 s warm (release).
      Parity on the author's machine (thisMonth): opencode identical (43 entries, 1,963,623
      output). claude/codex differ ON PURPOSE: the legacy reader stops at an 8 MB read budget
      per agent and flags `Approximate`, so it undercounted (claude 474 vs 18,482 entries;
      codex 6 vs 78). Rust reads the full window. Also fixed: legacy per-project buckets
      ignored the window. **Gap:** price table only knows `claude-opus-5-5` and `gpt-5.6-luna`,
      so claude/codex cost shows "no data" for the models actually used → O5c.
- [x] **O5b — Usage panel in the app** (render `get_usage`, window selector, per agent and
      per project). `usageWindow` config field (default thisMonth), pure view-model in
      `app/src/usage-view.ts`, loading/error states, stale-response guard, no polling.
      Route: delegated writer. Evidence: RED (unknown field) → GREEN; `cargo test` 51/51
      (parent re-ran), clippy clean, `tsc --noEmit` clean (parent re-ran), `npm run build` OK,
      `tauri dev` launched without errors. **Gaps:** no frontend test runner (view-model
      untested); UI not visually inspected by the agent → user smoke pending.
- [ ] **O5c — Pricing that is real and customizable**: verified per-model prices plus a
      user override file, so cost is never invented and anyone can add their models.
- [ ] **O5 — Usage readers in Rust** (umbrella) (claude jsonl, codex jsonl, opencode sqlite, pricing
      table), per agent and per project, configurable time window; wired to the
      `agent-usage` panel. Parity with the legacy widget numbers on the author's machine.
- [ ] **O6 — Docs and architecture rewrite** for the product: core + readers + extensions,
      quick start per OS, how to add a custom metric.
- [ ] **O8 — Remove everything old** (user, 2026-09-28: "todo lo que sea viejo lo borramos...
      debe quedar lo más clean posible el repo").
      - [x] Stale `HKCU\...\Run\Hotbar` autostart entry removed (pointed to a deleted exe).
      - [x] Finished-feature trackers `odd/tasks/{herdr-hub,hotbar-widget,omniroute-autofallback}.md`
            and the Herdr-only `docs/status-panes.md` deleted (kept in git history and Engram).
      - [ ] After O5b reaches parity on the author's machine: delete `legacy/windows-widget/`,
            `extensions/omniroute/legacy-widget/`, and `%APPDATA%\com.hotbar.app` (only after
            the new app has migrated the config).
      - [ ] O6 rewrites `README.md`, `docs/architecture.md`, `docs/hotbar.md`, `app/README.md`.
- [ ] **O7 — Engram project migration** `herdr-omniroute` → `orbitbar` (after the folder
      rename; verify the supported mechanism first).

## Checks

- TDD: strict (session config). Runner: `cargo test` in `hotbar-tauri/src-tauri` (Rust);
  `tsc --noEmit` for the frontend. The legacy PowerShell widget has no test suite: its checks
  are parse (`[Parser]::ParseFile`) plus a manual smoke of the usage panel.
- Every task: no personal data in `git diff` (grep for user paths, machine name, `sk-`).

## Delivery

- Forecast ~2000 authored changed lines (O5 alone ~900) → above the 400-line budget.
  Strategy: `ask-on-risk` (default); chain strategy pending the user's choice.
- Known history issue: the key-like string is already in git history on `main`. It is
  corrupted (not the valid key), but rewriting history is a separate, destructive decision
  left to the user.

## Progress

- 2026-09-28: mapping done (delegated explorer). Branch created. Doc created.

## Next step

O1 (personal data), delegated writer.
