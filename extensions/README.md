# Extensions

orbitbar's core depends on no external service or program: its base metric is
token spend per agent (claude/codex/opencode) and per project, read only from
the agents' own local files. Everything under `extensions/` is optional - the
core bar, the Tauri app and their tests all work with this directory removed.

An extension adds a signal from a program the user may not have installed
(here, the OmniRoute gateway and, through it, Herdr). Core code never imports
from `extensions/`; the reverse is fine.

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

`omniroute.ts` - a pi (gentle-pi) extension: `/omniroute` slash command,
footer status, post-call warning. Requires pi; loaded from pi's own
extensions directory, not from this repo directly. It talks to the OmniRoute
gateway directly, independent of Herdr.

**Enable:** copy or symlink `omniroute.ts` into pi's user extensions
directory. It resolves the repo root from its own file location, so it keeps
reading `OMNIROUTE_NODE` / `OMNIROUTE_ENTRY` from this repo's `.env`
wherever pi loads it from.
