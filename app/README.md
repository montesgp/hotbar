# Orbitbar (app)

The Tauri 2 app: a Rust core and a vanilla TypeScript frontend. This is the
whole product; see the [root README](../README.md) for installation and usage
and [docs/architecture.md](../docs/architecture.md) for how the pieces fit
together.

## Stack

- **Tauri 2** (Rust and the system webview): a transparent, frameless,
  always-on-top window. The bar renders in a webview and talks to Rust
  commands over `invoke()`.
- **Vanilla TypeScript and CSS**, no framework. Theme tokens arrive as CSS
  custom properties (`--ob-*`) computed from the Rust `ThemePalette`, so a
  theme is data, not code.
- **rusqlite with the `bundled` feature**, so SQLite is part of the binary.

## Layout

```
app/
  index.html              entry page (bar, panel and menu containers)
  src/main.ts             loads config, applies theme, renders items, drag, collapse, menu
  src/usage-view.ts       pure usage-panel view-model (no DOM)
  src/styles.css          bar, panel and menu styling driven by --ob-* properties
  src-tauri/
    src/lib.rs            Tauri commands, applying window placement, autostart sync
    src/placement.rs      pure window-placement rules (unit tested)
    src/config.rs         config schema, loader, theme palettes
    src/usage/            per-agent readers and pricing (see architecture.md)
    tauri.conf.json       window flags and bundle config
  pricing.example.json    template for a user pricing.json override
```

## Development

Prerequisites per OS (Rust with MSVC and the C++ Build Tools on Windows, Xcode
Command Line Tools on macOS, WebKitGTK 4.1 development packages on Linux) are
listed in the [root README](../README.md#build-from-source).

```sh
npm install
npm run tauri dev     # dev loop (Vite and reload)
npm run tauri build   # installers and bundles for the current OS
```

A debug build never registers the OS autostart entry, so development runs do
not leave anything behind at login.

## Checks

```sh
cd src-tauri && cargo test                          # Rust unit tests
cd src-tauri && cargo clippy --all-targets -- -D warnings
npx tsc --noEmit                                    # frontend type check (from app/)
npm run build                                       # type check and Vite build (from app/)
```

## Config

The per-user `config.json` (location per OS and every field are documented in
the [root README](../README.md#configuration)) is created on first launch and
saved when you change a setting. A minimal example:

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

Two themes ship: `dark` (the default) and `light`. They share one crescent
shape and differ only in color. Any other name resolves to `dark`, so a typo
never breaks the bar.

## Distribution

`npm run tauri build` produces regular binaries and installers with no runtime
to install beyond the OS webview: Windows NSIS `-setup.exe` and `.msi`, macOS
`.app` and `.dmg`, Linux `.deb`, `.rpm` and `.AppImage`. Output lands in
`src-tauri/target/release/bundle/`. Windows is verified manually; macOS and
Linux bundles are built by CI. CI (`.github/workflows/ci.yml`) checks every
pull request on all three OSes, and pushing a `vX.Y.Z` tag runs the Release
workflow (`.github/workflows/release.yml`), which publishes unsigned
installers to GitHub Releases. See the release process in
[CONTRIBUTING.md](../CONTRIBUTING.md#release-process).

Always build releases through the Tauri CLI (`npm run tauri build`, or
`npx tauri build --no-bundle` for the binary only). A plain
`cargo build --release` skips Tauri's `custom-protocol` feature: the binary
then loads the Vite dev server URL instead of the embedded frontend and shows
an unstyled page.

## Build output

`src-tauri/target/` is Cargo's build cache (git-ignored). It grows to several
GB because it keeps compiled dependencies for fast rebuilds, split into
`debug/` (dev runs and tests) and `release/`. Only these paths are meant to be
used:

| Path | What it is |
| --- | --- |
| `target/release/bundle/` | Installers to share or install (`nsis/`, `msi/` on Windows; `dmg/`, `macos/` on macOS; `deb/`, `rpm/`, `appimage/` on Linux) |
| `target/release/orbitbar(.exe)` | The standalone release binary |

Install Orbitbar from its installer for daily use, so autostart points at the
installed copy. `cargo clean` (from `src-tauri/`) then reclaims all of
`target/` safely; the next build recreates it.
