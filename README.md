# orbitbar

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

A cross-platform, always-on-top desktop bar that shows what your local AI
coding agents are actually costing you — per agent, per project, for the time
window you pick. Built with [Tauri 2](https://tauri.app/) (Rust + TypeScript),
runs on Windows, macOS and Linux.

> Screenshot: coming soon.

## Why

Claude Code, Codex CLI and OpenCode each keep a local history of every
session — tokens in, tokens out, which project. That data already exists on
disk; orbitbar just reads it and turns it into a number you can glance at
without opening a billing dashboard that doesn't exist for a CLI agent.

## Features

- **Always-on-top crescent.** Docks to the right edge of your primary
  monitor, collapses to a small tab, can be dragged, and opens a usage panel
  to the left.
- **Token usage and cost**, per agent (Claude Code, Codex, OpenCode) and per
  project, over Today / 7 days / 30 days / This month.
- **Local only.** Reads local session files. No network calls, no accounts,
  no telemetry.
- **Real prices, never invented ones.** A built-in, sourced price table with
  a per-user override file for models it doesn't know yet; a model with no
  price entry shows "no price data" instead of a guess.
- **Configurable.** Theme, items, autostart, usage window, and monitor
  placement all live in one JSON config file.

## Quick start (from source)

Prerequisites: Node.js, a Rust toolchain, and the OS-level webview
dependencies from the
[Tauri prerequisites guide](https://tauri.app/start/prerequisites/).

```sh
cd app
npm install
npm run tauri dev     # run in development, with hot reload
npm run tauri build   # produce an installer/binary for the current OS
```

`npm run tauri build` produces an NSIS installer and an MSI on Windows.
macOS (`.dmg`/`.app`) and Linux (`.deb`/`.AppImage`) builds are expected to
work through Tauri's own bundler but have not yet been verified in CI — see
[Roadmap](#roadmap).

## Configuration

orbitbar creates a config file with defaults on first launch and rewrites it
whenever you change a setting from the app:

| OS | Path |
| --- | --- |
| Windows | `%APPDATA%\com.orbitbar.app\config.json` |
| Linux | `~/.config/com.orbitbar.app/config.json` |
| macOS | `~/Library/Application Support/com.orbitbar.app/config.json` |

Key fields: `theme` (`classic` or `dark`), `fontSize`, `monitor`, `margin`,
`collapsed`, `autoStart` (on by default), `usageWindow` (persists your last
selector), and `items` — the cells rendered on the bar, each with an `id`,
`glyph`, `label`, `tooltip` and `action`.

The default items open the usage panel per agent, a combined usage view, the
config editor, and an autostart toggle:

| Action | What it does |
| --- | --- |
| `agent-usage` | Opens the usage panel with all three agents. |
| `agent-usage:claude` / `agent-usage:codex` / `agent-usage:opencode` | Opens the panel scoped to one agent. |
| `toggle-autostart` | Flips the autostart setting and reconciles the OS entry. |
| `edit-config` | Opens `config.json` in the OS default editor (opener plugin, scoped to the app config dir). |
| `run:<command>`, `omniroute-status` | Reserved item actions; not yet wired up in the Tauri app (a clicked cell shows a placeholder — see O9 in `odd/tasks/orbitbar-rebrand.md` for `run:<command>`'s open security decision). |

A hand-edited config that fails to parse never bricks the bar: the broken
file is renamed to `config.json.invalid` and the app restarts on defaults.

### Controls

Right-click the bar (or the collapsed tab) for a menu: open config.json or
pricing.json in your editor, collapse/expand, reload config.json without
restarting, or quit. Escape closes the menu, same as it closes the usage
panel; clicking outside it closes it too.

### Usage panel

The panel selector switches the window; totals and a per-project breakdown
(top projects by output tokens) render below it. A rough text mock of what
one agent's block looks like:

```text
Claude Code                              This month ▾
------------------------------------------------------
output tokens   211,300      cost  ~$53.04 (API-equiv.)
------------------------------------------------------
project              output tokens        cost
orbitbar                   180,400       ~$45.10
some-other-repo             30,900        ~$7.94
```

- `~$X (API-equiv.)` — Claude Code and Codex CLI subscriptions are not
  billed per token, so their cost is an estimate: real token counts priced
  at the provider's public API rate.
- A plain `$X` with no marker — OpenCode reports its own real cost; it is
  shown as-is.
- `no price data` — the model has no entry in the price table (built-in or
  your override) and orbitbar refuses to guess.

## Pricing and custom models

Built-in prices live in `app/src-tauri/src/usage/pricing.rs`, one row per
model with a `source` URL and an `as_of` date. To add or override a model,
copy [`app/pricing.example.json`](app/pricing.example.json) to
`pricing.json` next to `config.json` (same directory as the table above).
Matching is by exact model id, then longest id prefix, so a future dated
suffix (`claude-opus-5-5-20260926`) still resolves against a shorter entry
without an edit. The file is reloaded on every panel refresh — no restart
needed — and a malformed `pricing.json` falls back to the built-in table
plus a warning, never a crash.

## Privacy

orbitbar reads local files only:

| Agent | Source | Access |
| --- | --- | --- |
| Claude Code | `~/.claude/projects/**/*.jsonl` | Read-only |
| Codex CLI | `~/.codex/sessions/**/*.jsonl` | Read-only |
| OpenCode | `~/.local/share/opencode/opencode.db` | Read-only SQLite |

No network requests, no external services, no accounts. See
[docs/architecture.md](docs/architecture.md) for the exact reader
implementation.

## Extensions

Everything under [`extensions/`](extensions/README.md) is optional and adds
signals from programs you may not have installed (the Herdr TUI plugin, the
OmniRoute gateway integration). The core bar has no dependency on them and
works with the directory removed. See
[extensions/README.md](extensions/README.md).

## Roadmap

- Set up CI to build and verify macOS and Linux installers (only the
  Windows build has been exercised by the author so far; nothing in the
  codebase is Windows-specific).
- Distribution through package managers (winget, Homebrew, an AppImage feed)
  — planned, not yet done. For now, build from source.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE) © 2026 montesgp
