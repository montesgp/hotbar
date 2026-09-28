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
- [ ] **O4 — Rename hotbar → orbitbar.** Folders, Tauri identifier/productName, Cargo and npm
      package names, UI copy, config paths. Identifier change moves the config dir
      (`com.hotbar.app` → `com.orbitbar.app`): migrate the existing config on first launch.
- [ ] **O5 — Usage readers in Rust** (claude jsonl, codex jsonl, opencode sqlite, pricing
      table), per agent and per project, configurable time window; wired to the
      `agent-usage` panel. Parity with the legacy widget numbers on the author's machine.
- [ ] **O6 — Docs and architecture rewrite** for the product: core + readers + extensions,
      quick start per OS, how to add a custom metric.
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
