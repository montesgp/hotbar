# AGENTS.md

Guide for AI coding agents helping someone with Orbitbar. Humans should start
at [README.md](README.md); this file routes an agent to the right path fast
and points at the source of truth instead of repeating it.

Orbitbar is a Tauri 2 desktop bar (Rust backend, TypeScript + Vite frontend)
that shows token usage and cost for Claude Code, Codex and OpenCode, read
from their local session files. Windows, macOS and Linux.

## 1. Pick the path

Ask the person which one applies if it is not clear:

| The person wants to | Path |
| --- | --- |
| Use Orbitbar as is | [Use it](#2-use-it-no-code-changes) — no toolchain needed |
| A binary built from the current source, no changes | [Build it](#3-build-it-from-source) |
| Change Orbitbar or contribute | [Develop it](#4-develop-it) |

## 2. Use it (no code changes)

Do not clone or build anything.

1. Download the installer for their OS from
   <https://github.com/montesgp/orbitbar/releases/latest>. The file names per
   OS are in [README.md § Download and install](README.md#download-and-install).
2. The installers are unsigned. Walk them through the first-run prompt for
   their OS: [README.md § First run](README.md#first-run-unsigned-installers).
3. Agents are optional. Orbitbar reports any of Claude Code, Codex or
   OpenCode that is not installed; nothing else needs to be installed.
4. For settings, point them at [README.md § Configuration](README.md#configuration).
   The bar's right-click menu opens `config.json` and reloads it without a
   restart.

## 3. Build it from source

Prerequisites per OS are in
[README.md § Build from source](README.md#build-from-source). Check what is
already there before installing anything:

```sh
node -v    # any current LTS
rustc -V   # stable; on Windows the MSVC toolchain (stable-msvc)
cargo -V
```

- **Windows:** the Visual Studio C++ Build Tools ("Desktop development with
  C++") need an admin, GUI install. Ask the person to do it; do not try to
  script it silently.
- **Linux (Debian/Ubuntu):** the exact `apt` command is in the README.
- **macOS:** `xcode-select --install`.

Then:

```sh
cd app
npm ci
npm run tauri build                 # installers under src-tauri/target/release/bundle/
npx tauri build --no-bundle         # or: only the binary, src-tauri/target/release/orbitbar(.exe)
```

The first build compiles every Rust dependency and takes several minutes;
later builds are incremental.

## 4. Develop it

```sh
cd app
npm ci
npm run tauri dev    # hot reload
```

- Layout: [app/README.md § Layout](app/README.md#layout).
- How the usage readers, pricing and config work:
  [docs/architecture.md](docs/architecture.md).
- Checks, branches, commit style and the release process:
  [CONTRIBUTING.md](CONTRIBUTING.md).

Before reporting work as done, run the same checks CI runs (`npm run build`
first, because the Rust build embeds `app/dist`):

```sh
cd app
npx tsc --noEmit
npm run build
cd src-tauri
cargo test
cargo clippy --all-targets -- -D warnings
```

There is no frontend test runner yet. Keep `app/src/usage-view.ts` free of
DOM code so it stays unit-testable.

## Rules that are easy to get wrong

- **Never build with plain `cargo build --release`.** It skips Tauri's
  `custom-protocol` feature and the binary shows an unstyled page. Always go
  through `npm run tauri build` or `npx tauri build --no-bundle`.
- **Debug builds never register autostart.** Test "Start with system" only
  with a release build, ideally an installed one, because the OS entry points
  at the exact executable path that registered it.
- **Orbitbar only reads agent data.** The files in
  [README.md § Data sources](README.md#data-sources) are opened read-only.
  Do not add writes to them or any network calls.
- **Version bumps touch four files**, kept equal; see
  [CONTRIBUTING.md § Release process](CONTRIBUTING.md#release-process).
  Releases run only when a `vX.Y.Z` tag is pushed.
- **Pull requests target `dev`**, never `main`. `dev` reaches `main` through
  a promotion pull request.
- **`app/src-tauri/target/` is only a build cache.** It can grow to several
  GB; `cargo clean` in `app/src-tauri/` reclaims it and the next build
  recreates it. It holds the person's binary if they run Orbitbar from
  there, so ask before deleting it.
- **`extensions/` is optional** (Herdr and OmniRoute integrations) and not
  needed for the bar. See [extensions/README.md](extensions/README.md).
