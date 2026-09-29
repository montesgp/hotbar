# Contributing

## Development setup

Install the build prerequisites for your OS (see
[Build from source](README.md#build-from-source)), then:

```sh
cd app
npm ci
npm run tauri dev
```

`app/README.md` describes the project layout.

## Checks

CI runs these on Windows, macOS and Linux for every pull request. Run them
before opening one (`npm run build` first, because the Rust build embeds
`app/dist`):

```sh
cd app
npm ci
npx tsc --noEmit                                        # frontend type check
npm run build                                           # type check and Vite build
cd src-tauri
cargo test                                              # Rust unit tests
cargo clippy --all-targets -- -D warnings
```

There is no frontend test runner yet; `app/src/usage-view.ts` is kept free of
DOM code so it can be unit tested once one is added.

## Branches

- Create a feature branch (`feat/...`, `fix/...`, `chore/...`) and open a pull
  request against `dev`.
- Releases go from `dev` to `main` through a pull request.

```
feature branch ──► dev ──► main (tagged vX.Y.Z)
```

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

## Release process

1. On a branch from `dev`, bump the version in all four places, keeping them
   equal:
   - `app/package.json` (and `app/package-lock.json`, via `npm install`)
   - `app/src-tauri/tauri.conf.json`
   - `app/src-tauri/Cargo.toml`
   - `app/src-tauri/Cargo.lock` (updated by running `cargo check` in
     `app/src-tauri`)
2. Merge the bump into `dev`, then merge `dev` into `main` through a pull
   request once CI is green.
3. Tag the release on `main` and push the tag:

   ```sh
   git checkout main && git pull
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

4. The **Release** workflow (`.github/workflows/release.yml`) builds the
   installers for Windows, macOS (universal) and Linux with
   `tauri-apps/tauri-action` and publishes them as the GitHub Release
   "Orbitbar vX.Y.Z". The installers are unsigned. Check the release page
   afterwards for the expected assets.
