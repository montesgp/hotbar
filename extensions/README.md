# Extensions

Extensions add optional integrations to Orbitbar. Both are off by default; the
bar, the app and its tests work the same with or without this directory.

The core code never imports from `extensions/`; extensions may depend on the
repository (for example its `.env` file).

| Extension | What it adds | Needs |
| --- | --- | --- |
| [`herdr/`](#herdr) | OmniRoute gateway status, start and dashboard actions inside the Herdr TUI | Herdr, PowerShell, an OmniRoute install; currently Windows only |
| [`omniroute/`](#omniroute) | `/omniroute` command, footer status and post-call warning for the pi coding agent | pi, an OmniRoute install |

## Shared configuration

Both extensions read their settings from a `.env` file at the repository root.
Copy [`.env.example`](../.env.example) to `.env` and set:

| Variable | Meaning |
| --- | --- |
| `OMNIROUTE_NODE` | Path to the Node.js executable that launches the gateway. Defaults to `node` on `PATH`. |
| `OMNIROUTE_ENTRY` | Path to the OmniRoute CLI entry point (`omniroute.mjs`). Required: the start action reports a clear error when it is missing. |

`.env` is gitignored.

## Herdr

[`herdr/herdr-plugin.toml`](herdr/herdr-plugin.toml) is a Herdr plugin
manifest. It registers four actions (status, start gateway, open dashboard,
open status popup) and one popup pane, each backed by a PowerShell script in
[`herdr/scripts/`](herdr/scripts). The manifest declares `platforms =
["windows"]` and `min_herdr_version = "0.7.0"`.

**Enable:**

1. Create `.env` as described above.
2. Register the `extensions/herdr` folder as a plugin in Herdr, so that Herdr
   loads `herdr-plugin.toml` from it. Refer to Herdr's own documentation for
   the plugin registration step.
3. The actions then appear in Herdr under the "OmniRoute" titles.

Herdr loads the plugin once the folder is registered.

## OmniRoute

[`omniroute/omniroute.ts`](omniroute/omniroute.ts) is an extension for the pi
coding agent. It adds a `/omniroute` slash command, a footer status and a
post-call warning, and talks to the OmniRoute gateway directly.

**Enable:** copy or symlink `omniroute.ts` into pi's user extensions
directory. The file resolves the repository root from its own location, so it
keeps reading this repository's `.env` wherever pi loads it from.
