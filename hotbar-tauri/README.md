# hotbar (Tauri)

Cross-platform rewrite of the classic PowerShell/XAML hotbar widget: a collapsed,
always-on-top, frameless taskbar launcher docked to the right edge of the active
monitor.

## Stack

- **Tauri 2** (Rust + system webview) — transparent, frameless, always-on-top
  window; icons and actions render in a webview and talk to Rust commands.
- **Vanilla TypeScript + CSS** frontend — no framework; theme tokens arrive as CSS
  custom properties from the Rust `ThemePalette`, so a theme is data, never code.

## Layout

```
hotbar-tauri/
  index.html             entry page (bar + cells container)
  src/main.ts            loads config, applies palette, renders items
  src/styles.css         bar/cell styling driven by --hb-* custom properties
  src-tauri/
    src/lib.rs           Tauri commands (get_config, save_config), window placement
    src/config.rs        config schema v2, loader, theme palettes
    tauri.conf.json      window flags + bundle config
```

## Config

Per-user config file, created on first launch with defaults, auto-saved on write:

- Windows: `%APPDATA%\com.hotbar.app\config.json`
- Linux: `~/.config/com.hotbar.app/config.json`
- macOS: `~/Library/Application Support/com.hotbar.app/config.json`

```json
{
  "monitor": "primary",
  "margin": 8,
  "collapsed": false,
  "theme": "classic",
  "fontSize": 10.0,
  "items": [ { "id": "claude", "label": "claude", "glyph": "0x2733", "action": "agent-usage:claude", "tooltip": "..." } ]
}
```

Themes currently shipped: `classic` (the original WPF look) and `dark`. An unknown
theme name falls back to `classic` so a typo never breaks the bar.

## Development

Prerequisites: Rust (MSVC toolchain on Windows), Node.js, and the OS webview
dependencies ([Tauri prerequisites](https://tauri.app/start/prerequisites/)).

```sh
npm install
npm run tauri dev     # dev loop (Vite + reload)
npm run tauri build   # produce installers/binaries for the current OS
```

## Distribution

End users never install Rust or a webview toolchain. The build produces regular
binaries/installers per OS: Windows `-setup.exe`/`.msi` (uses the preinstalled
WebView2), macOS `.dmg`/`.app`, Linux `.deb` or single-file `.AppImage`; GitHub
Actions builds and attaches them to Releases.