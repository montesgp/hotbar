# Extensions

orbitbar's core depends on no external service or program: its base metric is
token spend per agent (claude/codex/opencode) and per project, read only from
the agents' own local files. Everything under `extensions/` is optional - the
core bar, the Tauri app and their tests all work with this directory removed.

An extension adds a signal from a program the user may not have installed
(here, the OmniRoute gateway and, through it, Herdr). Core code never imports
from `extensions/`; the reverse is fine and already happens (the OmniRoute
hotbar readers dot-source the shared SQLite/process helpers that live in
`hotbar/lib`, because the core usage reader needs the exact same helpers for
its own opencode reader).

## extensions/herdr

The Herdr plugin manifest (`herdr-plugin.toml`) and its action scripts
(`scripts/`): status, start and dashboard commands for the OmniRoute gateway,
registered as a plugin inside Herdr. Requires Herdr itself; without it this
directory is simply never loaded.

**Enable:** install/point Herdr at this repository so it picks up
`extensions/herdr/herdr-plugin.toml`. The scripts read `OMNIROUTE_NODE` /
`OMNIROUTE_ENTRY` from a repo-root `.env` (see `.env.example`); without a
configured `OMNIROUTE_ENTRY` the start action fails with a clear message
instead of guessing a path.

## extensions/omniroute

Two independent OmniRoute integrations, grouped by the OmniRoute gateway they
talk to (not by Herdr, which neither of them depends on):

- `omniroute.ts` - a pi (gentle-pi) extension: `/omniroute` slash command,
  footer status, post-call warning. Requires pi; loaded from pi's own
  extensions directory, not from this repo directly.
- `hotbar/Get-OmniRouteStatus.ps1`, `hotbar/Get-OmniRouteCombos.ps1` - readers
  for the legacy PowerShell widget's `omniroute-status` panel item (gateway
  UP/DOWN, configured combos).

**Enable (hotbar widget):** the files already being present in
`extensions/omniroute/hotbar/` is the whole switch - `hotbar/hotbar.ps1`
detects them at startup and loads them. With them missing, an item whose
`action` is `omniroute-status` in `hotbar/config.json` still renders; it shows
"extension not installed" instead of crashing.

**Enable (pi extension):** copy or symlink `omniroute.ts` into pi's user
extensions directory. It resolves the repo root from its own file location, so
it keeps reading `OMNIROUTE_NODE` / `OMNIROUTE_ENTRY` from this repo's
`.env` wherever pi loads it from.
