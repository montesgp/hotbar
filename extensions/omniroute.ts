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
// start from any working directory. The repo root is this file's parent
// directory's parent (extensions/../).
const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..")

// Minimal KEY=VALUE .env loader: no external dependency, comments (#) and
// blank lines ignored, never overrides a variable already set in the
// environment.
function loadDotEnv(path: string) {
  if (!existsSync(path)) return
  for (const line of readFileSync(path, "utf8").split(/\r?\n/)) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith("#")) continue
    const eq = trimmed.indexOf("=")
    if (eq === -1) continue
    const key = trimmed.slice(0, eq).trim()
    const value = trimmed.slice(eq + 1).trim()
    if (key && !(key in process.env)) process.env[key] = value
  }
}
loadDotEnv(join(REPO_ROOT, ".env"))

const PORT = 20128
const NODE = process.env.OMNIROUTE_NODE || "node"
const ENTRY = process.env.OMNIROUTE_ENTRY || "omniroute/bin/omniroute.mjs"

function checkPort(port: number, timeoutMs = 1500): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.connect({ host: "127.0.0.1", port }, () => { socket.destroy(); resolve(true) })
    socket.on("error", () => resolve(false))
    socket.setTimeout(timeoutMs, () => { socket.destroy(); resolve(false) })
  })
}

async function startGateway(): Promise<boolean> {
  if (await checkPort(PORT)) return true
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
        if (up) { await ctx.ui.notify(`OmniRoute already UP on :${PORT}`, "info") }
        else { await startGateway(); await ctx.ui.notify(`Starting OmniRoute on :${PORT} - recheck in ~15s`, "info") }
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
