# Contributing

## Running the app

Install the build prerequisites for your OS (see the
[README](README.md#install-and-run-on-your-os)), then:

```sh
cd app
npm install
npm run tauri dev
```

## Tests and checks

Run these from `app/` before opening a pull request:

```sh
cd src-tauri && cargo test                              # Rust unit tests
cd src-tauri && cargo clippy --all-targets -- -D warnings
npx tsc --noEmit                                        # frontend type check
npm run build                                           # type check and Vite build
```

There is no frontend test runner yet; `app/src/usage-view.ts` is kept free of
DOM code so it can be unit tested once one is added.

## Branches

Work flows through `dev` and `staging` to `main`:

```
dev ──► staging ──► main
```

- `dev`: active development. Open pull requests against it.
- `staging`: pre-release testing.
- `main`: stable releases.

## Commit messages

This repo uses [Conventional Commits](https://www.conventionalcommits.org/)
(`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`, ...). Keep the summary line
short and describe the "why" in the body when it is not obvious.

## Reporting an issue

Open a GitHub issue with:

- OS and version (Windows, macOS or Linux, with the build number if relevant).
- Orbitbar version, or the commit you built from.
- The agent or agents involved (Claude Code, Codex, OpenCode) if the report is
  about the usage panel.
- Steps to reproduce and what you expected instead.

Redact real token history, API keys and any file paths that include your
username before pasting.
