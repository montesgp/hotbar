import { invoke } from "@tauri-apps/api/core";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import {
  PhysicalPosition,
  PhysicalSize,
} from "@tauri-apps/api/dpi";
import {
  currentMonitor,
  getCurrentWindow,
  monitorFromPoint,
  type Monitor,
} from "@tauri-apps/api/window";
import {
  agentFromAction,
  buildPanelViewModel,
  isUsageAction,
  WINDOW_OPTIONS,
  type AgentSectionViewModel,
  type TimeWindow,
  type UsageSnapshot,
} from "./usage-view";

export interface ThemePalette {
  name: string;
  background: string;
  panel: string;
  text: string;
  textDim: string;
  hoverBg: string;
  hoverFg: string;
  radiusBar: number;
  radiusCell: number;
  radiusHandle: number;
  tabRadius: number;
  barBorder: string;
  gradientTop: string;
  gradientBottom: string;
  halfMoon: boolean;
  moonRadius: number;
}

export interface Item {
  id: string;
  label: string;
  glyph: string;
  action: string;
  tooltip: string;
}

export interface OrbitbarConfig {
  monitor: string;
  margin: number;
  collapsed: boolean;
  theme: string;
  fontSize: number;
  /** Desired autostart state. Rust reconciles the OS entry against it on start. */
  autoStart: boolean;
  /** Persisted usage-panel window selector (Today | 7 days | 30 days | This month). */
  usageWindow: TimeWindow;
  items: Item[];
}

export interface ConfigPayload {
  config: OrbitbarConfig;
  palette: ThemePalette;
}

/**
 * Physical window sizes, ported 1:1 from the measured WPF v1 window. The bar is
 * a 72x400 crescent; the collapsed state is a 46x46 tab. These MUST match the
 * constants in src-tauri/src/lib.rs, which sizes the window before the webview
 * paints — a mismatch shows up as a clipped crescent, not a layout bug.
 */
const SIZE_COLLAPSED = new PhysicalSize(46, 46);
const SIZE_EXPANDED = new PhysicalSize(72, 400);
const PANEL_WIDTH = 320;

const win = getCurrentWindow();

/** Current window size for a collapse/panel combination. */
function sizeFor(collapsed: boolean, panelOpen: boolean): PhysicalSize {
  if (collapsed) return SIZE_COLLAPSED;
  return new PhysicalSize(
    SIZE_EXPANDED.width + (panelOpen ? PANEL_WIDTH : 0),
    SIZE_EXPANDED.height,
  );
}

/** Map a Rust palette to CSS custom properties on :root. */
export function applyTheme(palette: ThemePalette): void {
  const root = document.documentElement;
  root.style.setProperty("--ob-bg", palette.background);
  root.style.setProperty("--ob-panel", palette.panel);
  root.style.setProperty("--ob-text", palette.text);
  root.style.setProperty("--ob-text-dim", palette.textDim);
  root.style.setProperty("--ob-hover-bg", palette.hoverBg);
  root.style.setProperty("--ob-hover-fg", palette.hoverFg);
  root.style.setProperty("--ob-press-bg", palette.hoverFg);
  root.style.setProperty("--ob-radius-cell", `${palette.radiusCell}px`);
  root.style.setProperty("--ob-radius-handle", `${palette.radiusHandle}px`);
  root.style.setProperty("--ob-tab-radius", `${palette.tabRadius}px`);
  root.style.setProperty("--ob-bar-border", palette.barBorder);
  root.style.setProperty("--ob-grad-top", palette.gradientTop);
  root.style.setProperty("--ob-grad-bottom", palette.gradientBottom);

  // The moon radii come from the window geometry, not from the palette.
  // rx must equal the bar width and ry the half height: anything that overflows
  // an edge makes CSS rescale the whole corner, which silently turns the
  // half-ellipse into a circle. The palette only decides moon on/off.
  //
  // The class goes on <body>, NOT on `root`: `root` is <html>, and the rule in
  // styles.css is `body.moon .bar`. Toggling it on <html> compiles fine and
  // silently never matches, which leaves the bar with the 22px fallback radius
  // and no crescent at all.
  root.style.setProperty("--ob-moon-rx", `${SIZE_EXPANDED.width}px`);
  root.style.setProperty("--ob-moon-ry", `${SIZE_EXPANDED.height / 2}px`);
  document.body.classList.toggle("moon", palette.halfMoon);
}

/**
 * Cells are glyph-only circles. The label stays in the DOM (tooltip target and
 * accessible name) but is never painted — the WPF bar showed no text and
 * painting it would break the silhouette.
 */
export function renderCells(container: HTMLElement, items: Item[]): void {
  container.replaceChildren();
  for (const item of items) {
    const cell = document.createElement("div");
    cell.className = "cell";
    cell.title = item.tooltip;
    cell.setAttribute("role", "button");
    cell.setAttribute("aria-label", item.tooltip || item.label);
    cell.dataset.id = item.id;
    cell.dataset.action = item.action;
    // Keep the native drag region from swallowing the click.
    cell.setAttribute("data-tauri-drag-region", "false");

    const glyph = document.createElement("span");
    glyph.className = "glyph";
    const code = Number.parseInt(item.glyph, 16);
    glyph.textContent = Number.isFinite(code)
      ? String.fromCodePoint(code)
      : item.glyph;
    cell.appendChild(glyph);

    const label = document.createElement("span");
    label.className = "label";
    label.textContent = item.label;
    cell.appendChild(label);

    container.appendChild(cell);
  }
}

/** Right-center a physical window on a monitor, honoring the config margin. */
async function snapToMonitor(
  monitor: Monitor,
  size: PhysicalSize,
  margin: number,
): Promise<void> {
  const x = monitor.position.x + monitor.size.width - size.width - margin;
  const y = monitor.position.y + (monitor.size.height - size.height) / 2;
  await win.setPosition(new PhysicalPosition(x, y));
}

/**
 * Resize + reposition the window for the requested collapse/panel state.
 *
 * The window is created `resizable: false` (there is no OS chrome to grab
 * with `decorations: false` anyway, so this only stops something else from
 * dragging an edge). On Windows that flag makes tao lock the window's
 * min/max inner size to whatever size it had at the moment `resizable`
 * turned false, and every `setSize` after that is silently clamped back to
 * that locked size — the bar never grows for the panel and the "moved but
 * still 72 wide" window a user sees is `snapToMonitor` positioning for the
 * size that was requested, not the size that was actually applied. Toggling
 * `setResizable` around the resize clears that lock, lets the real size
 * apply, then re-locks it at the new size so nothing else can drag it.
 */
async function applyState(
  cfg: OrbitbarConfig,
  panelOpen: boolean,
): Promise<void> {
  const size = sizeFor(cfg.collapsed, panelOpen);
  const monitor = await currentMonitor();
  if (!monitor) return;
  await win.setResizable(true);
  await win.setSize(size);
  await win.setResizable(false);
  await snapToMonitor(monitor, size, cfg.margin);
}

function setCollapsedUi(collapsed: boolean): void {
  document.body.classList.toggle("collapsed", collapsed);
  const collapse = document.querySelector<HTMLButtonElement>("#collapse");
  if (collapse) {
    // The chevron points where the motion goes: collapsing shrinks the bar
    // toward the screen edge (right), expanding grows it into the desktop (left).
    collapse.innerHTML = collapsed ? "&#8248;" : "&#8250;";
  }
}

function setPanelUi(open: boolean): void {
  const panel = document.querySelector<HTMLElement>("#panel");
  if (panel) panel.hidden = !open;
}

/**
 * Token-usage panel: renders `get_usage(window)` for either every agent
 * (`agent-usage`) or one agent (`agent-usage:<agent>`). Ported from the
 * legacy widget's `Get-OrbitbarAgentLines` / `Get-OrbitbarUsageLines`
 * (`legacy/windows-widget/orbitbar.ps1`): status, totals, cost, then up to 5
 * top projects. Unlike the legacy panel this has no refresh timer — the task
 * calls for a fetch on open and on window change only, never background
 * polling.
 */

/** Which agent filter (or "all") is currently shown, so a window-selector
 * change re-fetches the same view instead of resetting to "all agents". */
let usagePanelFilter: string | null = null;
/** Guards against a stale response winning a race when the panel is
 * reopened, or the window changed, before the previous fetch resolved. */
let usageFetchToken = 0;

function usageStatusBadge(status: AgentSectionViewModel["status"]): string {
  if (status.state === "ok") return "";
  return status.state === "notInstalled" ? " (not installed)" : " (error)";
}

/** One agent's block: name/status, totals, cost, then its top projects. */
function renderUsageSection(section: AgentSectionViewModel): HTMLElement {
  const el = document.createElement("div");
  el.className = "usage-section";

  const heading = document.createElement("div");
  heading.className = "usage-agent-name";
  heading.textContent = `${section.displayName}${usageStatusBadge(section.status)}`;
  el.appendChild(heading);

  if (!section.totals) {
    const line = document.createElement("div");
    line.className = "usage-line usage-line--dim";
    line.textContent = section.statusText;
    el.appendChild(line);
    return el;
  }

  const totals = section.totals;
  const tokenLine = document.createElement("div");
  tokenLine.className = "usage-line";
  tokenLine.textContent = `in ${totals.input}  out ${totals.output}  cache ${totals.cacheRead}/${totals.cacheWrite}`;
  el.appendChild(tokenLine);

  if (totals.reasoning) {
    const reasoningLine = document.createElement("div");
    reasoningLine.className = "usage-line usage-line--dim";
    reasoningLine.textContent = `reasoning ${totals.reasoning}  ·  ${totals.entries} entries`;
    el.appendChild(reasoningLine);
  } else {
    const entriesLine = document.createElement("div");
    entriesLine.className = "usage-line usage-line--dim";
    entriesLine.textContent = `${totals.entries} entries`;
    el.appendChild(entriesLine);
  }

  const costLine = document.createElement("div");
  costLine.className = "usage-line usage-cost";
  costLine.textContent = totals.cost;
  el.appendChild(costLine);

  if (section.topProjects.length > 0) {
    const projects = document.createElement("div");
    projects.className = "usage-projects";
    for (const project of section.topProjects) {
      const row = document.createElement("div");
      row.className = "usage-project-row";
      // The full normalized path, so a disambiguated or truncated name never
      // loses the real location.
      row.title = project.path;

      const name = document.createElement("span");
      name.className = "usage-project-name";
      name.textContent = project.name;
      row.appendChild(name);

      const stats = document.createElement("span");
      stats.className = "usage-project-stats";
      stats.textContent = `${project.output}  ${project.cost}`;
      row.appendChild(stats);

      projects.appendChild(row);
    }
    el.appendChild(projects);
  }

  return el;
}

/** The window selector row (Today / 7 days / 30 days / This month). */
function renderWindowSelector(
  cfg: OrbitbarConfig,
  onChange: (next: TimeWindow) => void,
): HTMLElement {
  const row = document.createElement("div");
  row.className = "usage-window-row";

  const select = document.createElement("select");
  select.className = "usage-window-select";
  select.setAttribute("aria-label", "Usage time window");
  for (const option of WINDOW_OPTIONS) {
    const opt = document.createElement("option");
    opt.value = option.value;
    opt.textContent = option.label;
    if (option.value === cfg.usageWindow) opt.selected = true;
    select.appendChild(opt);
  }
  select.addEventListener("change", () => {
    onChange(select.value as TimeWindow);
  });
  row.appendChild(select);

  return row;
}

function renderUsageLoading(body: HTMLElement, cfg: OrbitbarConfig, onWindowChange: (next: TimeWindow) => void): void {
  body.replaceChildren();
  body.appendChild(renderWindowSelector(cfg, onWindowChange));
  const loading = document.createElement("div");
  loading.className = "usage-line usage-line--dim";
  loading.textContent = "Loading usage…";
  body.appendChild(loading);
}

function renderUsageError(body: HTMLElement, cfg: OrbitbarConfig, onWindowChange: (next: TimeWindow) => void, message: string): void {
  body.replaceChildren();
  body.appendChild(renderWindowSelector(cfg, onWindowChange));
  const error = document.createElement("div");
  error.className = "usage-line usage-error";
  error.textContent = `Could not read usage: ${message}`;
  body.appendChild(error);
}

function renderUsageResult(
  body: HTMLElement,
  cfg: OrbitbarConfig,
  onWindowChange: (next: TimeWindow) => void,
  snapshot: UsageSnapshot,
  filterAgent: string | null,
): void {
  body.replaceChildren();
  body.appendChild(renderWindowSelector(cfg, onWindowChange));
  const vm = buildPanelViewModel(snapshot, filterAgent);
  if (vm.sections.length === 0) {
    const empty = document.createElement("div");
    empty.className = "usage-line usage-line--dim";
    empty.textContent = "No usage data for this selection.";
    body.appendChild(empty);
    return;
  }
  for (const section of vm.sections) {
    body.appendChild(renderUsageSection(section));
  }
}

/**
 * Opens (or re-fetches) the usage panel for `action` ("agent-usage" or
 * "agent-usage:<agent>"). `togglePanel` resizes/repositions the window;
 * `persistConfig` is passed in so a window-selector change survives restart.
 */
async function openUsagePanel(
  cfg: OrbitbarConfig,
  action: string,
  body: HTMLElement,
  togglePanel: (open: boolean) => Promise<void>,
): Promise<void> {
  const filterAgent = agentFromAction(action);
  usagePanelFilter = filterAgent;

  const fetchUsage = async (): Promise<void> => {
    const token = ++usageFetchToken;
    renderUsageLoading(body, cfg, onWindowChange);
    try {
      const snapshot = await invoke<UsageSnapshot>("get_usage", { window: cfg.usageWindow });
      if (token !== usageFetchToken) return; // superseded by a newer fetch
      renderUsageResult(body, cfg, onWindowChange, snapshot, usagePanelFilter);
    } catch (err) {
      if (token !== usageFetchToken) return;
      console.error("orbitbar: get_usage failed", err);
      renderUsageError(body, cfg, onWindowChange, String(err));
    }
  };

  function onWindowChange(next: TimeWindow): void {
    if (cfg.usageWindow === next) return;
    cfg.usageWindow = next;
    void persistConfig(cfg);
    void fetchUsage();
  }

  await togglePanel(true);
  await fetchUsage();
}

/** Persist the current config object back to disk. */
async function persistConfig(cfg: OrbitbarConfig): Promise<void> {
  try {
    await invoke("save_config", { cfg });
  } catch (err) {
    console.error("orbitbar: failed to save config", err);
  }
}

/**
 * Drag + snap. #bar carries data-tauri-drag-region so the runtime performs the
 * native OS drag synchronously inside the mousedown message (this is the
 * reliable cross-platform path; a manual IPC-driven move loses pointer events
 * the moment the cursor leaves the webview). We only track the gesture and
 * snap + persist when the user releases.
 */
function enableDrag(cfg: OrbitbarConfig): void {
  const bar = document.querySelector<HTMLElement>("#bar");
  if (!bar) return;

  let dragging = false;

  bar.addEventListener("pointerdown", (ev) => {
    const target = ev.target as HTMLElement;
    if (target.closest(".cell") || target.closest(".collapse")) return;
    if (target.closest(".tab")) return;
    if (ev.button !== 0) return;
    dragging = true;
  });

  // The OS drives the window during the native drag; when the user releases the
  // hwnd that held mouse capture fires this. Snap to the right edge of the
  // monitor under the window center and persist it.
  bar.addEventListener("pointerup", () => {
    if (!dragging) return;
    dragging = false;
    void (async () => {
      const pos = await win.outerPosition();
      const size = await win.outerSize();
      const cx = pos.x + size.width / 2;
      const cy = pos.y + size.height / 2;
      const monitor = await monitorFromPoint(cx, cy);
      if (!monitor) return;
      await snapToMonitor(monitor, size, cfg.margin);
      const monitorName = monitor.name || "primary";
      if (cfg.monitor !== monitorName) {
        cfg.monitor = monitorName;
        await persistConfig(cfg);
      }
    })();
  });
}

/**
 * The autostart cell is a stateful toggle, not a link, so it has to render its
 * current state. The bar is glyph-only and the glyph comes from the user's
 * config, so "off" is shown by dimming the cell and spelling the state out in
 * the tooltip rather than by swapping the glyph: swapping it here would fight
 * the config on the next render and silently drop the user's chosen icon.
 */
function applyAutostartUi(cell: HTMLElement, baseTooltip: string, enabled: boolean) {
  const state = enabled ? "autostart on" : "autostart off";
  cell.classList.toggle("cell--off", !enabled);
  const text = baseTooltip ? `${baseTooltip} · ${state}` : state;
  cell.title = text;
  cell.setAttribute("aria-label", text);
}

window.addEventListener("DOMContentLoaded", async () => {
  try {
    const payload = await invoke<ConfigPayload>("get_config");
    applyTheme(payload.palette);
    document.documentElement.style.setProperty(
      "--ob-font-size",
      `${payload.config.fontSize}px`,
    );
    const cfg = payload.config;

    const cells = document.querySelector<HTMLElement>("#cells");
    if (cells) renderCells(cells, cfg.items);

    // Autostart toggles: cell element -> the tooltip the config declared, kept
    // so the state suffix can be recomposed instead of appended twice.
    const autostartCells = new Map<HTMLElement, string>();
    if (cells) {
      for (const item of cfg.items) {
        if (item.action !== "toggle-autostart") continue;
        const cell = cells.querySelector<HTMLElement>(
          `.cell[data-id="${CSS.escape(item.id)}"]`,
        );
        if (cell) autostartCells.set(cell, item.tooltip);
      }
    }

    // Read the real OS registration, not the config. They can disagree: the
    // user can revoke the Run key in OS settings without touching our config.
    if (autostartCells.size > 0) {
      try {
        const on = await isAutostartEnabled();
        for (const [cell, tooltip] of autostartCells) {
          applyAutostartUi(cell, tooltip, on);
        }
      } catch (err) {
        console.error("orbitbar: could not read autostart state", err);
      }
    }

    let panelOpen = false;
    // The action the open panel is showing (a cell's `data-action`, or the
    // synthetic id below for the placeholder demo panel). Clicking the same
    // cell again, Escape, or the panel's close button all close the panel;
    // tracking this is what tells a second click on the same cell "close"
    // apart from "switch to this agent".
    let activePanelAction: string | null = null;
    setCollapsedUi(cfg.collapsed);
    setPanelUi(panelOpen);

    const togglePanel = async (open: boolean) => {
      panelOpen = open;
      if (!open) activePanelAction = null;
      setPanelUi(panelOpen);
      await applyState(cfg, panelOpen);
    };

    const closePanel = async () => {
      if (!panelOpen) return;
      await togglePanel(false);
    };

    const collapseBtn = document.querySelector<HTMLButtonElement>("#collapse");
    if (collapseBtn) {
      collapseBtn.addEventListener("click", async () => {
        cfg.collapsed = true;
        await togglePanel(false);
        setCollapsedUi(true);
        await applyState(cfg, false);
        await persistConfig(cfg);
      });
    }

    const tabBtn = document.querySelector<HTMLButtonElement>("#tab");
    if (tabBtn) {
      tabBtn.addEventListener("click", async () => {
        cfg.collapsed = false;
        setCollapsedUi(false);
        await applyState(cfg, panelOpen);
        await persistConfig(cfg);
      });
    }

    if (cells) {
      cells.addEventListener("click", async (ev) => {
        const target = (ev.target as HTMLElement).closest<HTMLElement>(".cell");
        if (!target) return;

        // A toggle acts in place. It must not open the panel, and it must not
        // report success it did not get: if the OS refuses the write we leave
        // both the config and the cell showing the old state.
        if (autostartCells.has(target)) {
          const tooltip = autostartCells.get(target) ?? "";
          const next = !cfg.autoStart;
          try {
            if (next) await enableAutostart();
            else await disableAutostart();
          } catch (err) {
            console.error("orbitbar: autostart could not be changed", err);
            applyAutostartUi(target, tooltip, cfg.autoStart);
            return;
          }
          cfg.autoStart = next;
          applyAutostartUi(target, tooltip, next);
          await persistConfig(cfg);
          return;
        }

        const action = target.dataset.action ?? "";

        // Clicking the cell that is already driving the open panel closes
        // it, same as Escape or the close button.
        if (panelOpen && activePanelAction === action) {
          await closePanel();
          return;
        }

        const body = document.querySelector<HTMLElement>("#panel-body");
        if (!body) return;

        if (isUsageAction(action)) {
          activePanelAction = action;
          await openUsagePanel(cfg, action, body, togglePanel);
          return;
        }

        // Other item actions (omniroute-status, run:cmd, edit-config, ...)
        // are wired up separately; a cell click just demonstrates panel
        // geometry until they land.
        activePanelAction = action;
        body.replaceChildren();
        const line = document.createElement("div");
        line.textContent = `${target.dataset.id}: ${action}`;
        body.appendChild(line);
        await togglePanel(true);
      });
    }

    const panelCloseBtn = document.querySelector<HTMLButtonElement>("#panel-close");
    if (panelCloseBtn) {
      panelCloseBtn.addEventListener("click", () => {
        void closePanel();
      });
    }

    window.addEventListener("keydown", (ev) => {
      if (ev.key !== "Escape") return;
      void closePanel();
    });

    enableDrag(cfg);
  } catch (err) {
    console.error("orbitbar: failed to load config", err);
  }
});
