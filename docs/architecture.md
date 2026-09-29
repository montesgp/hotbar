# Architecture

## What this is

Orbitbar is a Tauri 2 desktop app: a Rust core plus a TypeScript/HTML
frontend, packaged as one native binary per OS (Windows, macOS, Linux). It
reads the token-usage history that Claude Code, Codex and OpenCode write to
disk, prices it, and renders it in a small always-on-top bar, overall and per
project. Installation and usage are covered in the [root README](../README.md).

## How it fits together

```mermaid
flowchart LR
  subgraph FE["Frontend — app/src (TypeScript)"]
    direction TB
    MAIN["main.ts<br/>window state, theme, item clicks"]
    VIEW["usage-view.ts<br/>pure view-model, formatting"]
    MAIN --> VIEW
  end
  subgraph BE["Rust core — app/src-tauri/src"]
    direction TB
    CMD["Tauri commands<br/>get_config · save_config · get_usage<br/>get_config_path · ensure_pricing_file · quit_app"]
    CFG["config.rs<br/>schema, load/save, themes"]
    USAGE["usage/mod.rs<br/>collect_usage, TimeWindow"]
    PRICE["usage/pricing.rs<br/>built-in table + pricing.json"]
    CMD --> CFG
    CMD --> USAGE
    USAGE --> PRICE
  end
  subgraph READERS["Per-agent readers — app/src-tauri/src/usage"]
    direction TB
    RC["claude.rs"]
    RX["codex.rs"]
    RO["opencode.rs"]
  end
  subgraph SRC["Local files the agents wrote"]
    direction TB
    CC["~/.claude/projects/**/*.jsonl"]
    CX["~/.codex/sessions/**/*.jsonl"]
    OP["~/.local/share/opencode/opencode.db"]
  end

  MAIN -->|"invoke()"| CMD
  USAGE --> RC
  USAGE --> RX
  USAGE --> RO
  RC -->|"read-only"| CC
  RX -->|"read-only"| CX
  RO -->|"read-only SQLite"| OP
  CMD -->|"JSON payload"| MAIN
```

Every read is a local file, and the frontend talks to the Rust core only
through Tauri's `invoke()` IPC.

## Layers

| Layer | Component | Responsibility |
| --- | --- | --- |
| Window/UI | `app/src/main.ts`, `app/src/styles.css`, `app/index.html` | Crescent bar geometry, collapse/expand, drag, theme application, item clicks, panel and context-menu toggling |
| View-model | `app/src/usage-view.ts` | Pure functions that turn a `UsageSnapshot` into rendered lines — no DOM, unit-testable in isolation |
| Tauri commands | `app/src-tauri/src/lib.rs` | `get_config`, `save_config`, `get_usage`, `get_config_path`, `ensure_pricing_file`, `run_command`, `quit_app`; window placement and autostart reconciliation on launch |
| Launch actions | `app/src-tauri/src/launch.rs` | `run:<program> [args]`: parses the command line (whitespace-separated, double quotes group) and spawns the program directly, with no shell. `open:<url>` is handled in the frontend through the opener plugin, scoped to `http://` and `https://` in `capabilities/default.json` |
| Config | `app/src-tauri/src/config.rs` | Schema (`AppConfig`), per-OS config dir resolution, load/save, the `dark` and `light` theme palettes |
| Usage aggregation | `app/src-tauri/src/usage/mod.rs` | `TimeWindow`, `collect_usage`, project-path normalization, the `UsageSnapshot` shape returned to the frontend |
| Per-agent readers | `app/src-tauri/src/usage/{claude,codex,opencode}.rs` | One reader per agent store, each returning its own `AgentUsageReport` so a broken store never hides the other two |
| Pricing | `app/src-tauri/src/usage/pricing.rs` | Built-in price table (sourced, dated) plus `pricing.json` override loading and cost estimation |

## Interaction model

- **Usage panel.** Clicking an agent cell opens the panel to the left of the
  bar, always on the Today window. The window selector inside the panel is a
  session-only choice and is never written to `config.json`. Clicking the same
  cell again, pressing Escape or using the close button closes it.
- **Context menu.** A right click on the bar (or the collapsed tab), or a left
  click on the settings cell, opens a menu card inside the webview. The window
  grows to make room for it and shrinks back on close; clicking the settings
  cell again, clicking outside the card or pressing Escape closes it. Only one
  of the panel and the menu is open at a time. While the window is being
  resized the page is painted invisible (`body.resizing`) so intermediate
  frames never reach the screen.
- **Themes.** `dark` (default) and `light` are palettes in `config.rs`; the
  frontend maps them to `--ob-*` CSS custom properties. Choosing a theme in
  the menu saves it and re-applies the palette live. An unknown name resolves
  to `dark`.

## Data flow: opening the usage panel

1. The frontend calls `invoke("get_usage", { window })` for the selected
   `TimeWindow` (`today` / `last7Days` / `last30Days` / `thisMonth`),
   starting with `today`.
2. `get_usage` (in `lib.rs`) resolves the three store paths under the
   user's home directory, loads `pricing.json` if present, and runs the
   whole read off the main thread (`spawn_blocking`) so a large history
   never stalls the window.
3. `usage::collect_usage` calls each reader — `claude::read_usage`,
   `codex::read_usage`, `opencode::read_usage` — independently. Each
   returns an `AgentUsageReport` with its own `AgentStatus`
   (`Ok` / `NotInstalled` / `Error`), so one broken agent store never hides
   the other two.
4. Claude and Codex totals are priced through `pricing::estimate_cost`
   against the built-in table plus any `pricing.json` overrides; OpenCode
   already carries its own reported cost and skips pricing entirely.
5. The `UsageSnapshot` — window bounds, per-agent totals, per-project
   breakdown, an optional `pricingWarning` — serializes back to the
   frontend, where `usage-view.ts` turns it into the rendered panel.

## Adding a new agent reader

Each reader is self-contained and returns the same shape, so adding one
does not touch the other two:

1. Add `app/src-tauri/src/usage/<agent>.rs` with a
   `pub fn read_usage(root: &Path, start: DateTime<Local>, end: DateTime<Local>, ...) -> AgentUsageReport`,
   returning `AgentUsageReport::not_installed`, `::error`, or `::ok` as
   appropriate — never invent a status.
2. Register it in `usage/mod.rs`: a store-root field on `UsagePaths`, a
   resolution rule in `resolve_paths`, and a call inside `collect_usage`.
3. If the new agent reports cost per session already, skip pricing for it
   the way `opencode.rs` does; if it only reports tokens, price it through
   `pricing::estimate_cost` the way `claude.rs`/`codex.rs` do.
4. Add the agent to `app/src/usage-view.ts`'s types and rendering, and a
   default item (`agent-usage:<agent>`) in `config.rs::default_items` if it
   should get its own cell.

## Adding a custom metric

The same reader/command boundary is the extension point for a metric that
is something other than token usage: write a new Rust module with its own aggregation
function, expose it as a new `#[tauri::command]` in `lib.rs`, and add a
frontend view-model module (parallel to `usage-view.ts`) that turns the
returned JSON into panel lines. Nothing else in `main.ts` needs to change
beyond wiring the new item's `action` to the new `invoke()` call.

## Cross-platform notes

- Store paths are resolved relative to the home directory (`dirs::home_dir()`)
  the same way on every OS; only the per-user config directory differs
  (`app_config_dir()` from Tauri, which already accounts for the OS
  convention — `%APPDATA%`, `~/.config`, or `~/Library/Application Support`).
- SQLite access uses the bundled `rusqlite` (`features = ["bundled"]`), so
  SQLite is compiled into the binary and needs no system library on any OS.
- The window is transparent, frameless and always-on-top through Tauri's
  own window flags (`tauri.conf.json`), which map to the native APIs of
  each OS without extra code here.
- Autostart uses `tauri-plugin-autostart`, which registers the native
  mechanism per platform: a Registry Run entry on Windows, a LaunchAgent on
  macOS and an XDG autostart `.desktop` entry on Linux. `config.json`'s
  `autoStart` is the source of truth and is reconciled with the OS entry on
  every launch; a debug build never registers itself.
- The webview is WebView2 on Windows, WKWebView on macOS and WebKitGTK 4.1 on
  Linux.
- `pricing.json` lives next to `config.json` and is reloaded on every
  `get_usage` call. The "Open pricing file" menu entry creates it from a
  template if it is missing and opens it with the opener plugin, which the
  capabilities file scopes to the app config directory.

## Components in this repo

| Path | Purpose |
| --- | --- |
| `app/src/main.ts` | Window sizing/positioning, theme application, config load/save, item click handling |
| `app/src/usage-view.ts` | Usage panel view-model: formatting, window options, per-agent/per-project rendering |
| `app/src/styles.css` | Bar and panel styling, driven by `--ob-*` custom properties |
| `app/src-tauri/src/lib.rs` | Tauri commands, window placement, autostart reconciliation |
| `app/src-tauri/src/config.rs` | Config schema, load/save, theme palettes |
| `app/src-tauri/src/usage/mod.rs` | `TimeWindow`, `collect_usage`, `UsageSnapshot`, project-path normalization |
| `app/src-tauri/src/usage/claude.rs` | Claude Code JSONL reader |
| `app/src-tauri/src/usage/codex.rs` | Codex CLI JSONL reader |
| `app/src-tauri/src/usage/opencode.rs` | OpenCode SQLite reader |
| `app/src-tauri/src/usage/pricing.rs` | Built-in price table, `pricing.json` override loading, cost estimation |
| `extensions/` | Optional Herdr and OmniRoute integrations, see [extensions/README.md](../extensions/README.md) |
