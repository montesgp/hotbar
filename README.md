# Orbitbar

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

Orbitbar is a small floating bar that stays on top of your desktop and shows
the token usage and cost of your local AI coding agents (Claude Code, Codex
and OpenCode): overall, per project, and for the time window you pick. It
runs on Windows, macOS and Linux.

> Screenshot: coming soon.

## What you get

- **A bar that is always there.** It docks to the right edge of your primary
  monitor, collapses to a small tab, can be dragged, and opens a usage panel
  to its left.
- **Usage per agent and per project.** Output tokens and cost for Today,
  the last 7 days, the last 30 days or the current month.
- **Local data only.** Orbitbar reads the session history your agents
  already keep on disk. It needs no account, sends no telemetry and makes no
  network calls.
- **Sourced prices.** A built-in price table with a per-user override file.
  A model without a price entry shows `no price data`.
- **Light and dark themes**, switched live from the bar's menu.
- **One JSON config file** for items, theme, autostart and placement.

## Quick start

Orbitbar is built from source with Node.js and a Rust toolchain. Install the
prerequisites for your OS (below), then:

```sh
cd app
npm install
npm run tauri build   # installer or app bundle for the current OS
```

Use `npm run tauri dev` instead to run with hot reload while you work on it.

## Install and run on your OS

Agents are optional: Orbitbar reads whichever of Claude Code, Codex and
OpenCode are installed and reports the others as not installed. SQLite is
compiled into the binary, so there is nothing else to install on any OS.

### Windows

| | |
| --- | --- |
| Build prerequisites | [Node.js](https://nodejs.org/), [Rust](https://rustup.rs/) with the MSVC toolchain (`rustup default stable-msvc`), and the Visual Studio C++ Build Tools ("Desktop development with C++") |
| Runtime | WebView2, which ships with Windows 10 and 11 |
| Artifacts | NSIS installer (`*-setup.exe`) and `.msi` |
| Config | `%APPDATA%\com.orbitbar.app\config.json` |
| Autostart | A per-user Registry Run entry |

### macOS

| | |
| --- | --- |
| Build prerequisites | [Node.js](https://nodejs.org/), [Rust](https://rustup.rs/), and the Xcode Command Line Tools (`xcode-select --install`) |
| Runtime | WKWebView, part of macOS |
| Artifacts | `Orbitbar.app` and a `.dmg` |
| Config | `~/Library/Application Support/com.orbitbar.app/config.json` |
| Autostart | A LaunchAgent |

### Linux

| | |
| --- | --- |
| Build prerequisites | [Node.js](https://nodejs.org/), [Rust](https://rustup.rs/), and the WebKitGTK 4.1 development packages (see below) |
| Runtime | WebKitGTK 4.1 (`libwebkit2gtk-4.1-0` on Debian and Ubuntu) |
| Artifacts | `.deb`, `.rpm` and `.AppImage` |
| Config | `~/.config/com.orbitbar.app/config.json` |
| Autostart | An XDG autostart `.desktop` entry |

On Debian and Ubuntu, install the build dependencies with:

```sh
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

For Fedora, Arch and other distributions use the package list in the
[Tauri prerequisites guide](https://tauri.app/start/prerequisites/).

### Where the build output goes

`npm run tauri build` writes the installers under
`app/src-tauri/target/release/bundle/`, in one folder per format
(`nsis`, `msi`, `dmg`, `macos`, `deb`, `rpm`, `appimage`). Each OS produces
its own formats; build on the OS you are targeting.

### Platform status

| OS | Status |
| --- | --- |
| Windows | Built and tested. |
| macOS | Supported through Tauri's bundler; CI verification is planned. |
| Linux | Supported through Tauri's bundler; CI verification is planned. |

## Using Orbitbar

- **Click an agent cell** to open its usage panel. The panel always opens on
  Today; its selector switches to 7 days, 30 days or this month while the
  panel is open. Click the same cell again, press Escape, or use the close
  button to close it.
- **Right-click the bar** (or the collapsed tab), or **left-click the
  settings cell**, for the menu: edit `config.json`, open `pricing.json`,
  collapse or expand, reload the config without a restart, switch between
  Light and Dark, turn **Start with system** on or off, or quit. Escape or a click outside closes the menu.
- **Drag the bar** to move it; use the chevron at the top to collapse it to a
  tab.

A block in the panel looks like this:

```text
Claude Code                                 Today ▾
------------------------------------------------------
output tokens   211,300      cost  ~$53.04 (API-equiv.)
------------------------------------------------------
project              output tokens        cost
my-app                     180,400       ~$45.10
api-service                 30,900        ~$7.94
```

- `~$X (API-equiv.)` marks an estimate: Claude Code and Codex report token
  counts, and Orbitbar prices them at the provider's public API rate.
- A plain `$X` is a cost OpenCode reports itself, shown as-is.
- `no price data` means the model has no entry in the built-in table or your
  override.

## Configuration

Orbitbar creates `config.json` with defaults on first launch (paths per OS
above) and rewrites it when you change a setting from the app.

| Field | Meaning |
| --- | --- |
| `theme` | `dark` (default) or `light`. Any other value resolves to `dark`. |
| `fontSize` | Base font size in pixels. |
| `monitor` | Monitor to dock on; `primary` by default. |
| `margin` | Gap in pixels between the bar and the screen edge. |
| `collapsed` | Whether the bar starts as the small tab. |
| `autoStart` | Start Orbitbar at login (on by default; release builds only). The menu's **Start with system** entry changes it for you: its check mark shows the real OS registration, and selecting it updates the OS entry and this field. |
| `items` | The cells on the bar, each with `id`, `glyph`, `label`, `tooltip` and `action`. |

Item actions:

| Action | What it does |
| --- | --- |
| `agent-usage` | Opens the usage panel with all three agents. |
| `agent-usage:claude`, `agent-usage:codex`, `agent-usage:opencode` | Opens the panel for one agent. |
| `toggle-autostart` | Turns autostart on or off and updates the OS entry. |
| `edit-config` | Opens the bar's menu at the click; "Edit config" there opens `config.json` in your default editor. |
| `open:<url>` | Opens an `http://` or `https://` URL in your default browser. Other schemes are rejected and the panel shows why. |
| `run:<program> [args]` | Starts a program with its arguments when you click the cell. See below. |
| `omniroute-status` | Reserved name. A clicked cell shows a placeholder. |

`run:` splits the text after the colon on whitespace into a program and its
arguments; double quotes group an argument that contains spaces. Orbitbar
starts the program directly, without a shell, so `&&`, `|`, `>` and `%VAR%`
are passed to the program as plain text. The program runs detached: Orbitbar
does not wait for it, and it keeps running if you quit the bar. It starts only
when you click the cell. If it cannot be started, the panel shows the error.
A console program opens its own console window; a graphical program opens
only its own window.

```json
{ "id": "editor", "glyph": "0x270E", "label": "editor", "tooltip": "Open my project", "action": "run:code \"C:/Projects/my app\"" }
```

| OS | Example action |
| --- | --- |
| Windows | `run:notepad.exe`, or `run:code C:\Projects\my-app` |
| macOS | `run:open -a Terminal` (`open` is a program on macOS) |
| Linux | `run:code ~/projects/my-app` (a `~` is not expanded without a shell; use the full path) |

The default bar includes a `github` cell that uses `open:` to open the
Orbitbar repository. A `config.json` you already have is not rewritten, so add
the cell there by hand if you want it.

If `config.json` cannot be parsed, Orbitbar renames it to
`config.json.invalid` and starts on the defaults.

### Custom model prices

Built-in prices live in `app/src-tauri/src/usage/pricing.rs`, one row per
model with a source URL and an as-of date. To add or override a model, choose
"Open pricing file" in the bar's menu, which creates `pricing.json` next to
`config.json` from a template, or copy
[`app/pricing.example.json`](app/pricing.example.json) there yourself.
Matching is by exact model id, then by the longest id prefix, so a dated
model id still resolves against a shorter entry. The file is reloaded on every
panel refresh; a malformed `pricing.json` falls back to the built-in table
with a warning.

## Data sources

| Agent | Location read | Access |
| --- | --- | --- |
| Claude Code | `~/.claude/projects/**/*.jsonl` | Read-only |
| Codex | `~/.codex/sessions/**/*.jsonl` | Read-only |
| OpenCode | `~/.local/share/opencode/opencode.db` | Read-only SQLite |

`~` is your home directory on every OS. See
[docs/architecture.md](docs/architecture.md) for how the readers work.

## Extensions

Orbitbar has optional extensions for the Herdr TUI and the
OmniRoute gateway. They are off by default and add no requirements to the
bar. See [extensions/README.md](extensions/README.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
