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
- [x] **O5c — Pricing that is real and customizable**: verified per-model prices plus a
      user override file, so cost is never invented and anyone can add their models.
      Built-in rows carry `source`/`as_of` (Anthropic pricing page; OpenAI
      developers.openai.com/api/docs/pricing, 2026-09-28). Claude cache writes priced 5m
      (1.25×) vs 1h (2×). `pricing.json` next to config.json overrides/extends (longest
      prefix), malformed → `pricingWarning` + built-ins; example in `app/pricing.example.json`.
      `costBasis` apiEquivalent (claude/codex, shown "≈ $X API") vs reported (opencode).
      All 12 model ids in the author's data now priced. Route: delegated writer. Evidence:
      `cargo test` 69/69 (parent re-ran), clippy clean, `tsc` + `npm run build` OK; real data
      thisMonth: claude ≈ $1,157.97 API-equivalent, codex ≈ $88.84, opencode $0.00 reported.
      TDD note: RED was observed only as a wrong fixture expectation, not a clean
      missing-behavior failure — recorded honestly.
      Also: `c6a3a63` fix(autostart) — dev builds never register; re-enable heals stale paths.
- [x] **O5d — Panel bug from the user's smoke test** (2026-09-28): `$` did not widen the
      window and the user got stuck. Root cause: `resizable: false` makes Windows lock
      min/max size at creation, so `setSize(392x400)` was silently clamped to 72x400.
      Fix: toggle resizable around `setSize` (+ `core:window:allow-set-resizable`); panel
      moved out of the crescent container into its own card (it was clipped by the curve);
      close via same cell, Escape or ×; colliding project names get a parent segment, full
      path in the tooltip. Evidence: GetWindowRect 72x400 ↔ 392x400 right-edge snapped;
      screenshots of closed / open / switch / close states inspected (parent re-checked two);
      `cargo test` 69/69, clippy, `tsc`, `npm run build` clean. Lesson: O5b was marked done
      without opening the panel — UI tasks now require screenshot verification.
- [ ] **O9 — Wire the remaining item actions**: `edit-config` (open config.json and
      pricing.json in the OS default editor) and `run:<cmd>`; today they show a placeholder.
- [ ] **O5 — Usage readers in Rust** (umbrella) (claude jsonl, codex jsonl, opencode sqlite, pricing
      table), per agent and per project, configurable time window; wired to the
      `agent-usage` panel. Parity with the legacy widget numbers on the author's machine.
- [x] **O6 — Docs and architecture rewrite** for the product: core + readers + extensions,
      quick start per OS, how to add a custom metric. README, docs/architecture.md (Mermaid,
      reader/metric extension points), app/README.md, new CONTRIBUTING.md; docs/hotbar.md
      deleted. Route: delegated writer in a worktree (parallel with O5d). GitHub repo renamed
      `montesgp/hotbar` → `montesgp/orbitbar` with a new description and topics. Remaining
      hotbar/PowerShell mentions are code comments about the config migration and the
      legacy parity source (go away with O8).
- [x] **O8 — Remove everything old** (user, 2026-09-28: "todo lo que sea viejo lo borramos...
      debe quedar lo más clean posible el repo").
      - [x] Stale `HKCU\...\Run\Hotbar` autostart entry removed (pointed to a deleted exe).
      - [x] Finished-feature trackers `odd/tasks/{herdr-hub,hotbar-widget,omniroute-autofallback}.md`
            and the Herdr-only `docs/status-panes.md` deleted (kept in git history and Engram).
      - [x] `legacy/windows-widget/` and `extensions/omniroute/legacy-widget/` deleted
            (`20283d0`, work unit 1 below); `com.hotbar.app` config-dir migration removed from
            `config.rs` with its 3 tests. `%APPDATA%\com.hotbar.app` on disk is user machine
            state, not repo content — left for the user to clear manually if wanted.
      - [x] O6 rewrites `README.md`, `docs/architecture.md`, `docs/hotbar.md`, `app/README.md`.
            Evidence: `git grep -in "legacy|hotbar|powershell" -- . ':!odd'` → only justified
            hits left (BOM/Notepad-PowerShell-5.1 compat notes in config.rs, one past-tense
            History mention in architecture.md with no dead path, and the still-live
            `extensions/herdr/scripts/*.ps1` files).
- [ ] **O7 — Engram project migration** `herdr-omniroute` → `orbitbar` (after the folder
      rename; verify the supported mechanism first).

## Polish work units (2026-09-28, branch `chore/orbitbar-polish`)

- [x] **Unit 1 — Remove the legacy PowerShell widget** (user: "borremos el legacy").
      `git rm -r legacy extensions/omniroute/legacy-widget`; removed the gitignored
      `legacy/windows-widget/config.json` from disk and its `.gitignore` line; removed
      `legacy_config_dir`/`migrate_legacy_config` and their 3 tests from `config.rs` (the
      author's machine already migrated). Reworded every comment/doc pointing at the deleted
      files (main.ts, usage-view.ts, styles.css, usage/{mod,claude,codex,opencode,pricing}.rs,
      docs/architecture.md, extensions/README.md, herdr-plugin.toml description) to describe
      current behavior, keeping the real rationale (dedup rules, windowing, pricing sourcing).
      Commit `20283d0`. Evidence: `cargo test` 66/66, `cargo clippy --all-targets` clean,
      `tsc --noEmit` clean, `npm run build` OK.
- [x] **Unit 2 — Fix the collapsed tab chevron.** It used U+2038 CARET ("‸", points up) instead
      of U+2039 ("‹", points left — the tab expands leftward). Fixed in `app/index.html` and
      `app/src/main.ts` `setCollapsedUi`. Commit `044188e`. Evidence: screenshot of the
      collapsed dev-build window inspected — chevron points left.
- [x] **Unit 3 — One usage total per project, not per subfolder** (user: "con el total por
      proyecto es suficiente"). Root cause: projects were keyed by the raw session cwd, so
      `app`, `app/src-tauri`, `.claude/worktrees/agent-*` showed as separate rows of the same
      repo. Added `resolve_project_root` (shared by claude/codex/opencode readers, cached per
      snapshot): walks up from a cwd to the nearest `.git` ancestor, resolves a worktree's
      `.git` FILE to the main repo root, falls back to the cwd when no `.git` ancestor exists
      or the path is gone. Bounded at the user's home directory — a bare dotfiles repo directly
      in `$HOME` would otherwise merge every repo-less project into one row (caught by a real
      test failure on the author's machine, fixed before landing). Frontend name-collision
      disambiguation in `usage-view.ts` kept (different repos can still share a folder name).
      Commit `d10e977`. Evidence: RED → GREEN for 5 new `resolve_project_root` unit tests plus
      3 "two cwds/worktrees merge into one row" integration tests (one per reader); `cargo test`
      74/74, clippy clean. Real-data screenshot: `herdr-omniroute` appears as one row per agent
      (no `app`/`src-tauri` subfolder rows); see Verification below for the full row list.
- [x] **Unit 4 — Style the panel scrollbar to match the bar.** Added `scrollbar-width: thin` /
      `scrollbar-color` (Firefox/standard) and `::-webkit-scrollbar*` (WebView2/WebKit) rules
      to `.panel` using the existing `--ob-text-dim` / `--ob-hover-fg` tokens: 7px, rounded
      thumb, transparent track. Commit `da47a67`. Evidence: `tsc --noEmit` + `npm run build`
      OK; screenshot of a scrolled panel inspected — thin styled thumb visible, no content
      hidden under it.

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

- Chain strategy (user, 2026-09-28): every PR targets `dev`; promote `dev` → `main` at the
  end. Slice 1 = PR #1 `feat/orbitbar` → `dev` (autostart, O1–O4, O5a, O5b; `a80178a`..
  `8e7359b`). `dev` fast-forwarded to local `main` (`64d5e52`) before the PR.

## Progress

- 2026-09-28: mapping done (delegated explorer). Branch created. Doc created.
- 2026-09-28: `chore/orbitbar-polish` branch (based on `dev`) — bounded writer completed the
  4 polish units above (O8 now fully done). Verification: `cargo test` 74/74,
  `cargo clippy --all-targets` clean, `npx tsc --noEmit` clean, `npm run build` OK. Screenshots
  taken against a `npm run tauri dev` instance (PID distinguished from the user's pre-existing
  `target\release\orbitbar.exe`, which was never touched) and inspected: collapsed tab, `$`
  panel scrolled/unscrolled. Dev processes stopped after verification
  (`taskkill /T /F` on the npm→tauri→cargo→orbitbar.exe tree only).
  Per-project rows observed on the author's machine (ThisMonth), names only:
  - Claude: incoders-commerce, receipt-risk-detector, incoders-hive, herdr-omniroute,
    agent-a089898dab7b534a6 (5 rows)
  - Codex: incoders-commerce, portfolio, digital-menu, patri, personal (5 rows)
  - OpenCode: herdr-omniroute, patri, montesgp, digital-menu, portfolio (5 rows)
  No `app`/`src-tauri`/worktree subfolder rows appear for `herdr-omniroute` — confirms Unit 3.
  `agent-a089898dab7b534a6` stayed a separate row under Claude, consistent with the documented
  fallback (its session cwd has no reachable `.git` ancestor on this machine, e.g. a deleted
  worktree checkout).

## Next step

O9 (wire edit-config / run:<cmd> actions) or O7 (Engram project migration), whichever the
user wants next.
