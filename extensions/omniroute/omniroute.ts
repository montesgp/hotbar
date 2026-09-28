/**
 * OmniRoute gateway extension for pi — layer-exterior personalization.
 * Slash /omniroute [status|start|dashboard], footer status, post-call warning.
 * Survives pi/gentle-pi updates: lives in the user extensions dir only.
 */
import type { ExtensionAPI, ExtensionCommandContext } from "@earendil-works/pi-coding-agent"
import net from "node:net"
import { spawn } from "node:child_process"
import { readFileSync, existsSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"

// This file resolves its own location instead of relying on process.cwd(),
// because it runs as an extension loaded by a host (pi/gentle-pi) that may
// start from any working directory. The repo root is two levels up from this
// file's own directory (extensions/omniroute/../../, moved out of core in O3).
const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..")

// Minimal KEY=VALUE .env loader: no external dependency, comments (#) and
// blank lines ignored, never overrides a variable already set in the
// environment. A value may be wrapped in one pair of matching quotes
// ("..." or '...'), which is stripped - Windows paths with spaces are
// commonly quoted in a .env file.
function loadDotEnv(path: string) {
  if (!existsSync(path)) return
  for (const line of readFileSync(path, "utf8").split(/\r?\n/)) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith("#")) continue
    const eq = trimmed.indexOf("=")
    if (eq === -1) continue
    const key = trimmed.slice(0, eq).trim()
    let value = trimmed.slice(eq + 1).trim()
    if (value.length >= 2) {
      const first = value[0]
      const last = value[value.length - 1]
      if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
        value = value.slice(1, -1)
      }
    }
    if (key && !(key in process.env)) process.env[key] = value
  }
}
loadDotEnv(join(REPO_ROOT, ".env"))

const PORT = 20128
const NODE = process.env.OMNIROUTE_NODE || "node"
const ENTRY = process.env.OMNIROUTE_ENTRY

function checkPort(port: number, timeoutMs = 1500): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.connect({ host: "127.0.0.1", port }, () => { socket.destroy(); resolve(true) })
    socket.on("error", () => resolve(false))
    socket.setTimeout(timeoutMs, () => { socket.destroy(); resolve(false) })
  })
}

const ENTRY_MISSING_MESSAGE = "OMNIROUTE_ENTRY is not set (see .env.example)"

// Resolves false, never a guessed path: a wrong guess spawns "node" against a
// file that does not exist in the caller's cwd, which fails silently (the
// child is detached/unref'd) instead of telling the user what to fix.
async function startGateway(): Promise<boolean | typeof ENTRY_MISSING_MESSAGE> {
  if (await checkPort(PORT)) return true
  if (!ENTRY) return ENTRY_MISSING_MESSAGE
  return new Promise((resolve) => {
    const child = spawn(NODE, [ENTRY, "serve", "--daemon", "--no-open"], {
      detached: true,
      stdio: "ignore",
      windowsHide: true,
    })
    child.on("error", () => resolve(false))
    child.unref()
    resolve(true)
  })
}

export default function (api: ExtensionAPI) {
  api.registerCommand("omniroute", {
    description: "OmniRoute gateway: status, start, dashboard",
    handler: async (args: string | undefined, ctx: ExtensionCommandContext) => {
      const action = (args ?? "").trim().toLowerCase()
      const up = await checkPort(PORT)
      if (action === "start") {
        if (up) {
          await ctx.ui.notify(`OmniRoute already UP on :${PORT}`, "info")
        } else {
          const started = await startGateway()
          if (started === ENTRY_MISSING_MESSAGE) {
            await ctx.ui.notify(started, "warning")
          } else {
            await ctx.ui.notify(`Starting OmniRoute on :${PORT} - recheck in ~15s`, "info")
          }
        }
        return
      }
      if (action === "dashboard") {
        await api.exec("cmd", ["/c", "start", `http://localhost:${PORT}`])
        return
      }
      await ctx.ui.setStatus("omniroute", up ? `OmniRoute ● :${PORT}` : `OmniRoute ○ :${PORT}`)
      await ctx.ui.notify(up ? `OmniRoute UP on :${PORT}` : `OmniRoute DOWN on :${PORT}`, up ? "info" : "warning")
    },
  })

  api.on("session_start", async (ctx) => {
    const up = await checkPort(PORT)
    await ctx.ui.setStatus("omniroute", up ? `OmniRoute ● :${PORT}` : `OmniRoute ○ :${PORT}`)
  })

  api.on("after_provider_response", async (ctx, event: unknown) => {
    const status = (event as { status?: number } | null)?.status
    if (typeof status === "number" && status >= 400) {
      await ctx.ui.notify(`Provider responded ${status} - OmniRoute gateway may be down (check /omniroute)`, "warning")
    }
  })
}
