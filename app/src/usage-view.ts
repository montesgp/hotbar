/**
 * Pure formatting and view-model logic for the token-usage panel. Kept
 * separate from main.ts (DOM wiring) so the numbers can be unit tested once a
 * frontend test runner exists — none is wired up yet, see the O5b task notes.
 *
 * Mirrors the Rust shapes serialized by `get_usage`
 * (app/src-tauri/src/usage/mod.rs), camelCase throughout. The legacy
 * PowerShell widget (`legacy/windows-widget/orbitbar.ps1`,
 * `Get-OrbitbarAgentLines`) is the reference for what the panel shows: per
 * agent status, totals, cost, then up to 5 top projects by output tokens.
 */

export type TimeWindow = "today" | "last7Days" | "last30Days" | "thisMonth";

export interface WindowOption {
  value: TimeWindow;
  label: string;
}

/** In the order the panel's window selector renders them. */
export const WINDOW_OPTIONS: WindowOption[] = [
  { value: "today", label: "Today" },
  { value: "last7Days", label: "7 days" },
  { value: "last30Days", label: "30 days" },
  { value: "thisMonth", label: "This month" },
];

export type AgentStatus =
  | { state: "ok" }
  | { state: "notInstalled" }
  | { state: "error"; detail: string };

/**
 * Whether `cost` is money the provider actually reported (opencode's own
 * `session.cost`) or an estimate computed from official list prices times
 * real token counts (claude, codex). Codex usage read through a ChatGPT
 * subscription is not billed per token at all — `apiEquivalent` must never
 * be rendered as if it were a bill the user received.
 */
export type CostBasis = "apiEquivalent" | "reported";

export interface UsageTotals {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  reasoningTokens: number;
  cost: number | null;
  costBasis: CostBasis | null;
  entries: number;
}

export interface ProjectUsage {
  path: string;
  name: string;
  model: string;
  totals: UsageTotals;
}

export interface AgentUsageReport {
  agent: string;
  status: AgentStatus;
  totals: UsageTotals;
  projects: ProjectUsage[];
  detail: string;
}

export interface UsageSnapshot {
  window: TimeWindow;
  windowStart: string;
  windowEnd: string;
  generatedAt: string;
  agents: AgentUsageReport[];
  /** Set when `pricing.json` exists but failed to parse; built-ins were used anyway. */
  pricingWarning: string | null;
}

const AGENT_DISPLAY_NAMES: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  opencode: "OpenCode",
};

/** The label the panel shows for an agent id; unknown ids pass through unchanged. */
export function agentDisplayName(agent: string): string {
  return AGENT_DISPLAY_NAMES[agent] ?? agent;
}

/**
 * Compact token count: under 1000 renders as a plain integer, otherwise one
 * decimal with a K/M suffix (e.g. 97.1K, 5.25M — the task calls for a 3
 * significant-digit millions form, so M gets two decimals instead of one).
 */
export function formatCompactNumber(value: number): string {
  const abs = Math.abs(value);
  if (abs < 1000) return String(Math.trunc(value));
  if (abs < 1_000_000) {
    return `${(value / 1000).toFixed(1)}K`;
  }
  return `${(value / 1_000_000).toFixed(2)}M`;
}

/**
 * Cost in USD, two decimals. `null` means the store or the price table had no
 * data for that model — the panel must say so, never show 0 or invent a
 * number (see `UsageTotals.cost` in usage/mod.rs).
 *
 * `costBasis` controls the prefix: `"apiEquivalent"` (claude, codex) is an
 * estimate computed from official list prices, never a bill the user
 * actually received, so it renders as "≈ $12.34 API" rather than a plain
 * dollar figure that would look like a reported charge. `"reported"`
 * (opencode's own `session.cost`) renders as a plain "$1.23".
 */
export function formatCost(cost: number | null, costBasis: CostBasis | null): string {
  if (cost === null) return "no price data";
  if (costBasis === "apiEquivalent") return `≈ $${cost.toFixed(2)} API`;
  return `$${cost.toFixed(2)}`;
}

/** Human status line: "ok", "not installed", or "error: <detail>". */
export function statusLabel(status: AgentStatus): string {
  switch (status.state) {
    case "ok":
      return "ok";
    case "notInstalled":
      return "not installed";
    case "error":
      return `error: ${status.detail}`;
  }
}

export interface ProjectLineViewModel {
  name: string;
  output: string;
  cost: string;
}

export interface AgentSectionViewModel {
  agent: string;
  displayName: string;
  status: AgentStatus;
  statusText: string;
  totals: {
    output: string;
    input: string;
    cacheRead: string;
    cacheWrite: string;
    reasoning: string | null;
    entries: string;
    cost: string;
  } | null;
  topProjects: ProjectLineViewModel[];
}

/** Max projects shown per agent, ported from the legacy widget's `$script:AgentHistoryMaxProjectsShown`. */
export const MAX_PROJECTS_SHOWN = 5;

/**
 * Builds the display model for one agent report. `totals` is `null` when the
 * agent is not `ok`, since there is nothing honest to show beyond the status.
 * Projects are already sorted by output tokens descending by the Rust side
 * (`AgentUsageReport::ok`); this only truncates to the top 5.
 */
export function buildAgentSection(report: AgentUsageReport): AgentSectionViewModel {
  const isOk = report.status.state === "ok";
  return {
    agent: report.agent,
    displayName: agentDisplayName(report.agent),
    status: report.status,
    statusText: statusLabel(report.status),
    totals: isOk
      ? {
          output: formatCompactNumber(report.totals.outputTokens),
          input: formatCompactNumber(report.totals.inputTokens),
          cacheRead: formatCompactNumber(report.totals.cacheReadTokens),
          cacheWrite: formatCompactNumber(report.totals.cacheWriteTokens),
          reasoning:
            report.totals.reasoningTokens > 0
              ? formatCompactNumber(report.totals.reasoningTokens)
              : null,
          entries: formatCompactNumber(report.totals.entries),
          cost: formatCost(report.totals.cost, report.totals.costBasis),
        }
      : null,
    topProjects: isOk
      ? report.projects.slice(0, MAX_PROJECTS_SHOWN).map((p) => ({
          name: p.name,
          output: formatCompactNumber(p.totals.outputTokens),
          cost: formatCost(p.totals.cost, p.totals.costBasis),
        }))
      : [],
  };
}

export interface UsagePanelViewModel {
  window: TimeWindow;
  sections: AgentSectionViewModel[];
}

/**
 * Builds the panel view model for a snapshot. `filterAgent` narrows to a
 * single agent (the `agent-usage:<agent>` action); omitted or unmatched shows
 * every agent, same as the plain `agent-usage` action.
 */
export function buildPanelViewModel(
  snapshot: UsageSnapshot,
  filterAgent?: string | null,
): UsagePanelViewModel {
  const agents = filterAgent
    ? snapshot.agents.filter((a) => a.agent === filterAgent)
    : snapshot.agents;
  return {
    window: snapshot.window,
    sections: agents.map(buildAgentSection),
  };
}

/** Parses the agent id out of an `agent-usage` / `agent-usage:<agent>` action. Returns `null` for "all agents". */
export function agentFromAction(action: string): string | null {
  const idx = action.indexOf(":");
  if (idx === -1) return null;
  return action.slice(idx + 1) || null;
}

/** True for both the "all agents" and the per-agent usage actions. */
export function isUsageAction(action: string): boolean {
  return action === "agent-usage" || action.startsWith("agent-usage:");
}
