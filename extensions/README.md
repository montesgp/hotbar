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

### OmniRoute status in the Orbitbar panel

Orbitbar has no OmniRoute code. To see the gateway status in its panel, point a
cell at the Herdr status script with the generic `panel:` action, which runs a
program and shows its output. `-Once` makes the script print one frame and
exit, which is what `panel:` needs (it does not wait for a key). Add an item to
`config.json` with the path of your checkout (forward slashes work in
PowerShell, and Orbitbar splits the line on whitespace, so a path with spaces
must be wrapped in double quotes):

```json
{
  "id": "omniroute",
  "glyph": "0x25CE",
  "label": "omniroute",
  "tooltip": "OmniRoute gateway status",
  "action": "panel:powershell -NoProfile -ExecutionPolicy Bypass -File C:/path/to/orbitbar/extensions/herdr/scripts/status-dashboard.ps1 -Once"
}
```

Then use **Reload config** from the bar's menu. Clicking the cell runs the
script (no console window, 10 second limit) and shows the gateway state and
combos. The script's last lines mention a popup that closes with `q`; ignore
them, that text is for the Herdr popup. Windows only, like the script.

## OmniRoute

[`omniroute/omniroute.ts`](omniroute/omniroute.ts) is an extension for the pi
coding agent. It adds a `/omniroute` slash command, a footer status and a
post-call warning, and talks to the OmniRoute gateway directly.

**Enable:** copy or symlink `omniroute.ts` into pi's user extensions
directory. The file resolves the repository root from its own location, so it
keeps reading this repository's `.env` wherever pi loads it from.
