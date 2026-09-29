# orbitbar (app)

The Tauri 2 app: Rust core + vanilla TypeScript frontend. This is the
product; see the [root README](../README.md) for the overview and
[docs/architecture.md](../docs/architecture.md) for how the pieces fit
together.

## Stack

- **Tauri 2** (Rust + system webview) — transparent, frameless,
  always-on-top window; icons and actions render in a webview and talk to
  Rust commands over `invoke()`.
- **Vanilla TypeScript + CSS** frontend — no framework. Theme tokens arrive
  as CSS custom properties (`--ob-*`) computed from the Rust
  `ThemePalette`, so a theme is data, never code.

## Layout

```
app/
  index.html              entry page (bar + panel containers)
  src/main.ts              loads config, applies theme, renders items, drag/collapse
  src/usage-view.ts         pure usage-panel view-model (no DOM)
  src/styles.css            bar/panel styling driven by --ob-* custom properties
  src-tauri/
    src/lib.rs              Tauri commands: get_config, save_config, get_usage
    src/config.rs            config schema v2, loader, theme palettes
    src/usage/               per-agent readers + pricing (see architecture.md)
    tauri.conf.json          window flags + bundle config
  pricing.example.json      template for a user pricing.json override
```

## Config

Per-user config file, created on first launch with defaults, auto-saved on
write:

- Windows: `%APPDATA%\com.orbitbar.app\config.json`
- Linux: `~/.config/com.orbitbar.app/config.json`
- macOS: `~/Library/Application Support/com.orbitbar.app/config.json`

```json
{
  "monitor": "primary",
  "margin": 8,
  "collapsed": false,
  "theme": "dark",
  "fontSize": 10.0,
  "autoStart": true,
  "items": [
    { "id": "claude", "label": "claude", "glyph": "0x2733", "action": "agent-usage:claude", "tooltip": "..." }
  ]
}
```

Themes shipped: `dark` (the default) and `light`. Both use the same crescent
shape; only the colors differ. You can also switch from the bar's right-click
menu. Any other name, including the retired `classic`, resolves to `dark` so
a typo or an old config never breaks the bar.

## Development

Prerequisites: Rust (MSVC toolchain on Windows), Node.js, and the OS
webview dependencies ([Tauri prerequisites](https://tauri.app/start/prerequisites/)).

```sh
npm install
npm run tauri dev     # dev loop (Vite + reload)
npm run tauri build   # produce installers/binaries for the current OS
```

## Checks

```sh
cd src-tauri && cargo test   # Rust unit tests
npx tsc --noEmit             # frontend type check (run from app/)
```

## Distribution

End users never install Rust or a webview toolchain. `npm run tauri build`
produces regular binaries/installers per OS: Windows `-setup.exe` (NSIS)
and `.msi`, macOS `.dmg`/`.app`, Linux `.deb` or `.AppImage`. Only the
Windows build has been exercised by the author so far; nothing here is
Windows-specific, but macOS and Linux builds are not yet verified in CI.
