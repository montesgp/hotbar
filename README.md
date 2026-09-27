# hotbar

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

A small floating, always-on-top Windows bar — an elongated half-moon pinned to the
right edge of your primary monitor. Click a cell and it expands an inline panel to
the left: **how much your AI agents actually cost you this month**, per agent and
per open project, plus one cell with the
[OmniRoute](https://github.com/montesgp/omniroute) gateway status.

It is standalone: it is not a Herdr pane, it works whether or not Herdr is running,
and it reads only what the agents themselves already wrote to disk.

> **Windows v1, honestly.** This is a Windows tool — Windows PowerShell 5.1 and WPF.
> The cross-platform advance is architectural, not delivered: every data source is
> read through a small decoupled reader layer (`hotbar/lib/*.ps1`), so a future port
> swaps readers instead of rewriting the widget. See
> [docs/architecture.md](docs/architecture.md).

## The hotbar widget

```text
  ╭──────────╮
  │        ▸ │   collapsed: a single tab, 46 px
  ╰──────────╯
```

Expanded it is a 72 x 400 vertical half-moon, vertically centred, with the panel
opening to its left.

```powershell
.\hotbar\launch-hotbar.ps1
```

That is the whole install. No package manager, no build step, no runtime downloads:
Windows PowerShell 5.1, WPF and `sqlite3.exe` (with a DLL fallback) are already on
the box.

- **Always on top, frameless, no taskbar entry.** `AllowsTransparency`,
  `WindowStyle=None`, `ShowInTaskbar=false`.
- **Single instance.** A named mutex refuses a second bar. Force-killing the
  widget releases an abandoned mutex that the next launch recovers, so a crash
  never wedges it permanently.
- **Collapse and expand.** The chevron at the top collapses the bar to a 46 px
  semicircular tab; the tab expands it again.
- **Right-click** for a context menu: open the config, reload it, collapse, quit.
  `Escape` quits too.
- **DPI-aware.** Screen pixels are converted to WPF device-independent units via
  `Graphics.FromHwnd(IntPtr.Zero).DpiX`, so the bar lands in the same physical
  spot on a scaled monitor.

## Items and actions

`hotbar/config.json` is the whole configuration. Items are rendered in order and
each one carries a glyph, a label, a tooltip and an action:

| Action | What it does |
| --- | --- |
| `none` | The cell is a placeholder. It renders and does nothing. |
| `agent-usage` | Expands the usage panel: the month's totals per agent. |
| `agent-usage:claude` | Expands the per-agent panel for Claude Code (month + per open project). |
| `agent-usage:codex` | Same for Codex CLI. |
| `agent-usage:opencode` | Same for OpenCode. |
| `omniroute-status` | Expands the inline panel with the gateway snapshot (optional; only if you run OmniRoute). |
| `edit-config` | Opens `config.json` in the default editor. |
| `run: <command>` | Runs a command. `.cmd`/`.bat` targets go through `cmd.exe /d /c`; an optional `cwd` is honoured. |

Glyphs are written as `0xNNNN` code points rather than literal characters, so the
whole tree stays pure ASCII and a wrong glyph is a parse error instead of a
mojibake surprise:

```json
{ "id": "claude", "label": "Claude", "glyph": "0x2733", "action": "agent-usage:claude" }
```

## What the panels show

### Agent usage — honest money

Claude Code, Codex CLI and OpenCode each write their session history to disk. The
widget reads those stores read-only and renders the current month:

```text
  claude~ sep           codex~ sep            opencode sep
  mes: out 211,3k       mes: out 37,5k        mes: out 1,6M  $0.00
        $53.04 (est)         $0.28 (est)
```

Two markers keep the money honest:

| Marker | Meaning |
| --- | --- |
| `~` | The read was **partial** (a bounded read budget was reached). The month shown is a floor, not the total. |
| `(est)` | The cost is **estimated** from the model's official list prices. |

- **Claude Code and Codex CLI do not record cost.** `(est)` means the widget
  priced the session tokens at the model's official list price
  (see [docs/hotbar.md#costo-estimado](docs/hotbar.md#costo-estimado%3A-cuando-es-estimado-y-cuando-real) for the table).
- **OpenCode records real cost** in its SQLite store, so its panel shows the real
  number with no `(est)` marker.
- **A model outside the price table renders `sin datos`** instead of a guessed
  number. Unknown is never invented.
- The legend line (`(est) = costo estimado`) appears only when an estimate is
  actually shown; `~ = lectura parcial` only when a read was partial.

### Per open project

Below the month line, the panel breaks the month down by the projects Herdr has
open (read from Herdr's own `session.json`). Subject sessions under a repo's path
count toward that repo; unrelated paths render `sin datos`:

```text
  incoders-commerce 211,3k
        $53.04 (est)
  herdr-omniroute: sin datos
```

### OmniRoute gateway (optional cell)

The `omniroute-status` cell expands gateway UP/DOWN on `:20128`, the active combo
and the configured combos, read from the gateway's own SQLite. It is the same
read-only data path the legacy plugin popup uses. If you do not use OmniRoute,
drop the cell — the widget does not need it.

## Where the data comes from

The widget **never starts any agent or the OmniRoute CLI**. It reads files the
agents already wrote:

| Agent | Store | Read |
| --- | --- | --- |
| Claude Code | `~\.claude\projects\**\*.jsonl` | History + live session, read-only |
| Codex CLI | `~\.codex\sessions\<date>\*.jsonl` | History + live session, read-only |
| OpenCode | `~\.local\share\opencode\opencode.db` | SQLite via `sqlite3.exe`, `-readonly`, or `winsqlite3.dll` |
| Herdr open projects | `%APPDATA%\herdr\session.json` | Which repos get a per-project row |

Claude and Codex reads are bounded (a byte budget per panel), so a huge history can
never freeze the UI — and that is exactly when the `~` partial marker appears.

## Self test

```powershell
.\hotbar\launch-hotbar.ps1 -SelfTest
```

Prints one `HOTBAR_SELFTEST` line per check and returns a real exit code. It
parses the config, validates every glyph and action, loads the XAML, checks the
expanded/collapsed/panel geometry against the real screen, reads live agent data,
shows the window for a few hundred milliseconds and closes it from a
`DispatcherTimer` with a watchdog behind it. A self-test that cannot fail is
worthless, so the failure path is exercised too: an unsupported action or a bad
glyph is reported per item and exits 1.

Nothing in the widget can hang the caller, and no test leaves a window behind.

## Legacy optional surfaces

This repo also carries the older surfaces around OmniRoute. They stay supported
but are **optional**: the widget exists precisely because they are confined to a
terminal/session.

| Surface | What it is | Status |
| --- | --- | --- |
| Herdr plugin (`herdr.omniroute`) | Status/start/dashboard actions + an on-demand status popup in the Herdr TUI | Supported, optional |
| pi extension (`extensions/omniroute.ts`) | `/omniroute` command, footer status, warning on non-2xx | Supported, optional |
| Status popup | One snapshot of gateway UP/DOWN + combos, painted once, no auto-refresh, closes on `q`/Enter | Supported, optional |

Docs: [docs/hotbar.md](docs/hotbar.md) (widget guide),
[docs/architecture.md](docs/architecture.md) (layers),
[docs/status-panes.md](docs/status-panes.md) (the plugin popup pattern).

## Scope and lifetime

- **The bar is per-user and explicit.** It starts when you launch it and lives until
  you quit it. It is not a Herdr pane, so restoring a Herdr session never spawns it,
  and Herdr not running has no effect on it.
- **The widget only reads** agent and gateway data. It never touches a tool install
  directory, never writes to an agent store, and only rewrites `config.json` when
  you choose **Edit config** (your editor does it). Tool updates cannot break it.
- **A broken read renders a visible line** (`sin datos` / `no disponible`), never an
  empty panel — a failed read must not look like "nothing happened this month".

## Branches

Simple promotion flow — everything converges on `main`:

| Branch | Purpose |
| --- | --- |
| `dev` | Active development |
| `staging` | Pre-release testing |
| `main` | Stable release |

## Requirements

- Windows
- Windows PowerShell 5.1 (the widget is WPF and needs STA; the launcher handles it)
- `sqlite3.exe` on `PATH` for OpenCode and combo data, with a `winsqlite3.dll` fallback if absent
- OmniRoute reachable at `http://localhost:20128` — only for the optional gateway cell
- Herdr 0.7.0 or newer — only if you want the legacy plugin surface

## License

[MIT](LICENSE) © 2026 montesgp