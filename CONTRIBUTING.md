# Contributing

## Running the app

```sh
cd app
npm install
npm run tauri dev
```

Prerequisites: Node.js, a Rust toolchain, and the OS webview dependencies
from the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/).

## Tests and checks

Run these from `app/` before opening a pull request:

```sh
cd src-tauri && cargo test   # Rust unit tests
npx tsc --noEmit             # frontend type check
```

There is no frontend test runner wired up yet; `app/src/usage-view.ts` is
kept free of DOM code specifically so it can be unit tested once one is
added.

## Commit messages

This repo uses [Conventional Commits](https://www.conventionalcommits.org/)
(`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`, ...). Keep the summary
line short and describe the "why" in the body when it is not obvious.

## Reporting an issue

Open a GitHub issue with:

- OS and version (Windows/macOS/Linux, build number if relevant).
- orbitbar version (or the commit you built from).
- Which agent(s) are involved (Claude Code, Codex, OpenCode) if the report
  is about the usage panel.
- Steps to reproduce and what you expected instead.

Never paste real token history, API keys, or full file paths that include
your username — redact them first.
