import { invoke } from "@tauri-apps/api/core";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import { openPath } from "@tauri-apps/plugin-opener";
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

/** The only two themes; anything else in config.json resolves to dark in Rust. */
export type ThemeName = "light" | "dark";

export interface OrbitbarConfig {
  monitor: string;
  margin: number;
  collapsed: boolean;
  theme: string;
  fontSize: number;
  /** Desired autostart state. Rust reconciles the OS entry against it on start. */
  autoStart: boolean;
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

/**
 * The context menu is a card rendered inside the webview (see
 * `.context-menu` in styles.css), not the OS's own popup menu: it has to be
 * styled with the `--ob-*` tokens like the panel, which an OS-drawn menu
 * cannot be. A 72px (or 46px collapsed) window has no room to show it, so it
 * borrows the same trick `applyState` already uses for the usage panel —
 * temporarily widen the window leftwards, snap to the right edge, restore on
 * close. `CONTEXT_MENU_MIN_HEIGHT` only matters collapsed: the 46px-tall tab
 * window is shorter than the menu itself, so opening it while collapsed also
 * grows the height, not just the width.
 */
const CONTEXT_MENU_WIDTH = 170;
const CONTEXT_MENU_MIN_HEIGHT = 270;
/** Seven entries plus two separators, sized from `.context-menu`'s own CSS;
 * kept as a constant instead of measured so opening the menu never needs an
 * extra hidden-then-remeasure paint. */
const CONTEXT_MENU_HEIGHT_ESTIMATE = 246;

const win = getCurrentWindow();

/**
 * No default WebView2/browser context menu anywhere in the bar — the menu
 * below is the only one. `#bar`'s own `contextmenu` handler (added in
 * DOMContentLoaded) both prevents the default and opens ours; this window
 * listener is the app-wide backstop for every other element (the panel, the
 * menu card itself).
 */
window.addEventListener("contextmenu", (ev) => {
  ev.preventDefault();
});

/** Current window size for a collapse/panel/menu combination. Only one of
 * `panelOpen` / `menuOpen` is ever true at a time — opening the menu closes
 * the panel first, see `openContextMenu`. */
function sizeFor(collapsed: boolean, panelOpen: boolean, menuOpen: boolean): PhysicalSize {
  if (menuOpen) {
    const base = collapsed ? SIZE_COLLAPSED.width : SIZE_EXPANDED.width;
    const height = collapsed ? CONTEXT_MENU_MIN_HEIGHT : SIZE_EXPANDED.height;
    return new PhysicalSize(base + CONTEXT_MENU_WIDTH, height);
  }
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
  menuOpen: boolean,
): Promise<void> {
  const size = sizeFor(cfg.collapsed, panelOpen, menuOpen);
  const monitor = await currentMonitor();
  if (!monitor) return;
  await win.setResizable(true);
  await win.setSize(size);
  await win.setResizable(false);
  await snapToMonitor(monitor, size, cfg.margin);
}

/** One clickable row, or the literal string "separator" for a divider. */
interface ContextMenuAction {
  label: string;
  run: () => void | Promise<void>;
  /** Present only on the theme choices: true marks the one in effect. */
  checked?: boolean;
}

/**
 * The fixed entry list, in order. "Collapse"/"Expand" reflects `cfg.collapsed`
 * so the label always matches what the click will actually do, and the theme
 * choice in effect (`currentTheme`, the resolved palette name) carries a check.
 */
function buildContextMenuActions(
  cfg: OrbitbarConfig,
  handlers: {
    openConfig: () => void | Promise<void>;
    openPricing: () => void | Promise<void>;
    toggleCollapsed: () => void | Promise<void>;
    reloadConfig: () => void | Promise<void>;
    setTheme: (theme: ThemeName) => void | Promise<void>;
    quit: () => void | Promise<void>;
  },
  currentTheme: string,
): (ContextMenuAction | "separator")[] {
  return [
    { label: "Open config", run: handlers.openConfig },
    { label: "Open pricing file", run: handlers.openPricing },
    { label: cfg.collapsed ? "Expand" : "Collapse", run: handlers.toggleCollapsed },
    { label: "Reload config", run: handlers.reloadConfig },
    "separator",
    { label: "Light", checked: currentTheme === "light", run: () => handlers.setTheme("light") },
    { label: "Dark", checked: currentTheme === "dark", run: () => handlers.setTheme("dark") },
    "separator",
    { label: "Quit Orbitbar", run: handlers.quit },
  ];
}

/** Paints the menu card. `onSelect` runs before the action itself, so the
 * menu is always closed (and the window resized back down) whether the
 * action succeeds or fails. */
function renderContextMenu(
  el: HTMLElement,
  actions: (ContextMenuAction | "separator")[],
  onSelect: () => void,
): void {
  el.replaceChildren();
  for (const action of actions) {
    if (action === "separator") {
      const sep = document.createElement("hr");
      sep.className = "context-menu-separator";
      el.appendChild(sep);
      continue;
    }
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "context-menu-item";
    btn.setAttribute("data-tauri-drag-region", "false");
    if (action.checked === undefined) {
      btn.setAttribute("role", "menuitem");
      btn.textContent = action.label;
    } else {
      // Radio-style entry: the check slot is always present so labels stay
      // aligned whether or not this one is the current choice.
      btn.setAttribute("role", "menuitemradio");
      btn.setAttribute("aria-checked", String(action.checked));
      const mark = document.createElement("span");
      mark.className = "context-menu-check";
      mark.textContent = action.checked ? "✓" : "";
      btn.append(mark, action.label);
    }
    btn.addEventListener("click", () => {
      onSelect();
      void action.run();
    });
    el.appendChild(btn);
  }
}

/** Places the menu near the click, clamped so it never runs off the (now
 * widened) window — `windowHeight` is the size the window was just resized
 * to, not the size it had when the click happened. */
function positionContextMenu(el: HTMLElement, clickY: number, windowHeight: number): void {
  const maxTop = Math.max(6, windowHeight - CONTEXT_MENU_HEIGHT_ESTIMATE - 6);
  const top = Math.min(Math.max(clickY - 8, 6), maxTop);
  el.style.top = `${top}px`;
}

/** The shared fallback for an item action that has no real behavior yet
 * (`run:<cmd>`, and — until wired — the context menu's Open config / Open
 * pricing entries): show which id/action fired inside the panel instead of
 * doing nothing, so the wiring can be inspected before it lands. */
async function showActionPlaceholder(
  id: string,
  action: string,
  body: HTMLElement,
  togglePanel: (open: boolean) => Promise<void>,
): Promise<void> {
  body.replaceChildren();
  const line = document.createElement("div");
  line.textContent = `${id}: ${action}`;
  body.appendChild(line);
  await togglePanel(true);
}

function setCollapsedUi(collapsed: boolean): void {
  document.body.classList.toggle("collapsed", collapsed);
  const collapse = document.querySelector<HTMLButtonElement>("#collapse");
  if (collapse) {
    // The chevron points where the motion goes: collapsing shrinks the bar
    // toward the screen edge (right), expanding grows it into the desktop (left).
    collapse.innerHTML = collapsed ? "&#8249;" : "&#8250;";
  }
}

function setPanelUi(open: boolean): void {
  const panel = document.querySelector<HTMLElement>("#panel");
  if (panel) panel.hidden = !open;
}

/**
 * Token-usage panel: renders `get_usage(window)` for either every agent
 * (`agent-usage`) or one agent (`agent-usage:<agent>`): status, totals,
 * cost, then up to 5 top projects. There is no refresh timer — the panel
 * fetches on open and on window change only, never background polling.
 */

/** Which agent filter (or "all") is currently shown, so a window-selector
 * change re-fetches the same view instead of resetting to "all agents". */
let usagePanelFilter: string | null = null;
/** The selector's current window. Session-only: reset to Today whenever the
 * panel opens from closed, never written to config. */
let usageWindow: TimeWindow = "today";
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
    if (option.value === usageWindow) opt.selected = true;
    select.appendChild(opt);
  }
  select.addEventListener("change", () => {
    onChange(select.value as TimeWindow);
  });
  row.appendChild(select);

  return row;
}

function renderUsageLoading(body: HTMLElement, onWindowChange: (next: TimeWindow) => void): void {
  body.replaceChildren();
  body.appendChild(renderWindowSelector(onWindowChange));
  const loading = document.createElement("div");
  loading.className = "usage-line usage-line--dim";
  loading.textContent = "Loading usage…";
  body.appendChild(loading);
}

function renderUsageError(body: HTMLElement, onWindowChange: (next: TimeWindow) => void, message: string): void {
  body.replaceChildren();
  body.appendChild(renderWindowSelector(onWindowChange));
  const error = document.createElement("div");
  error.className = "usage-line usage-error";
  error.textContent = `Could not read usage: ${message}`;
  body.appendChild(error);
}

function renderUsageResult(
  body: HTMLElement,
  onWindowChange: (next: TimeWindow) => void,
  snapshot: UsageSnapshot,
  filterAgent: string | null,
): void {
  body.replaceChildren();
  body.appendChild(renderWindowSelector(onWindowChange));
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
 * A window-selector change re-fetches but is never persisted: the panel
 * starts on Today each time it opens (see `usageWindow`).
 */
async function openUsagePanel(
  action: string,
  body: HTMLElement,
  togglePanel: (open: boolean) => Promise<void>,
): Promise<void> {
  const filterAgent = agentFromAction(action);
  usagePanelFilter = filterAgent;

  const fetchUsage = async (): Promise<void> => {
    const token = ++usageFetchToken;
    renderUsageLoading(body, onWindowChange);
    try {
      const snapshot = await invoke<UsageSnapshot>("get_usage", { window: usageWindow });
      if (token !== usageFetchToken) return; // superseded by a newer fetch
      renderUsageResult(body, onWindowChange, snapshot, usagePanelFilter);
    } catch (err) {
      if (token !== usageFetchToken) return;
      console.error("orbitbar: get_usage failed", err);
      renderUsageError(body, onWindowChange, String(err));
    }
  };

  function onWindowChange(next: TimeWindow): void {
    if (usageWindow === next) return;
    usageWindow = next;
    void fetchUsage();
  }

  await togglePanel(true);
  await fetchUsage();
}

const contextMenuEl = document.querySelector<HTMLElement>("#context-menu");

/**
 * Opens the right-click menu at `clickY`: closes the panel (only one of the
 * two ever occupies the widened area), widens+resizes the window for the
 * menu, then paints and positions the card. `setMenuOpen` updates the
 * `menuOpen` flag declared in DOMContentLoaded so `sizeFor`/Escape/outside-
 * click all agree on the current state.
 */
async function openContextMenu(
  cfg: OrbitbarConfig,
  clickY: number,
  closePanel: () => Promise<void>,
  setMenuOpen: (open: boolean) => void,
  actions: (ContextMenuAction | "separator")[],
  onSelect: () => void,
): Promise<void> {
  if (!contextMenuEl) return;
  await closePanel();
  setMenuOpen(true);
  renderContextMenu(contextMenuEl, actions, onSelect);
  await applyState(cfg, false, true);
  const size = sizeFor(cfg.collapsed, false, true);
  positionContextMenu(contextMenuEl, clickY, size.height);
  contextMenuEl.hidden = false;
}

/** Closes the menu and restores the window to its plain collapsed/expanded
 * size (never back to the panel — closing the menu does not reopen it). */
async function closeContextMenu(cfg: OrbitbarConfig, setMenuOpen: (open: boolean) => void): Promise<void> {
  setMenuOpen(false);
  if (contextMenuEl) contextMenuEl.hidden = true;
  await applyState(cfg, false, false);
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

/** cell element -> the tooltip the config declared, kept so the state suffix
 * can be recomposed instead of appended twice. Recomputed by `reloadConfig`
 * too, since a reload replaces the cell elements wholesale. */
function bindAutostartCells(cells: HTMLElement, items: Item[]): Map<HTMLElement, string> {
  const map = new Map<HTMLElement, string>();
  for (const item of items) {
    if (item.action !== "toggle-autostart") continue;
    const cell = cells.querySelector<HTMLElement>(`.cell[data-id="${CSS.escape(item.id)}"]`);
    if (cell) map.set(cell, item.tooltip);
  }
  return map;
}

/** Reads the real OS registration, not the config — they can disagree: the
 * user can revoke the Run key in OS settings without touching our config. */
async function refreshAutostartUi(autostartCells: Map<HTMLElement, string>): Promise<void> {
  if (autostartCells.size === 0) return;
  try {
    const on = await isAutostartEnabled();
    for (const [cell, tooltip] of autostartCells) {
      applyAutostartUi(cell, tooltip, on);
    }
  } catch (err) {
    console.error("orbitbar: could not read autostart state", err);
  }
}

window.addEventListener("DOMContentLoaded", async () => {
  try {
    const payload = await invoke<ConfigPayload>("get_config");
    applyTheme(payload.palette);
    // The resolved palette name ("light" | "dark"), not cfg.theme: a stored
    // "classic" or a typo resolves to dark in Rust, and the menu's check
    // must show what is actually painted.
    let themeName = payload.palette.name;
    document.documentElement.style.setProperty(
      "--ob-font-size",
      `${payload.config.fontSize}px`,
    );
    const cfg = payload.config;

    const cells = document.querySelector<HTMLElement>("#cells");
    if (cells) renderCells(cells, cfg.items);

    let autostartCells = cells ? bindAutostartCells(cells, cfg.items) : new Map<HTMLElement, string>();
    await refreshAutostartUi(autostartCells);

    let panelOpen = false;
    // Only one of panelOpen / menuOpen is ever true: opening either closes
    // the other first (see openContextMenu and toggleCollapsed).
    let menuOpen = false;
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
      await applyState(cfg, panelOpen, menuOpen);
    };

    const closePanel = async () => {
      if (!panelOpen) return;
      await togglePanel(false);
    };

    const setMenuOpen = (open: boolean) => {
      menuOpen = open;
    };

    const toggleCollapsed = async () => {
      await closeContextMenu(cfg, setMenuOpen);
      if (cfg.collapsed) {
        cfg.collapsed = false;
        setCollapsedUi(false);
        await applyState(cfg, panelOpen, false);
      } else {
        cfg.collapsed = true;
        await togglePanel(false);
        setCollapsedUi(true);
        await applyState(cfg, false, false);
      }
      await persistConfig(cfg);
    };

    const collapseBtn = document.querySelector<HTMLButtonElement>("#collapse");
    if (collapseBtn) {
      collapseBtn.addEventListener("click", () => {
        void toggleCollapsed();
      });
    }

    const tabBtn = document.querySelector<HTMLButtonElement>("#tab");
    if (tabBtn) {
      tabBtn.addEventListener("click", () => {
        void toggleCollapsed();
      });
    }

    // `edit-config` (the ⚙ cell) and "Open config" both open config.json in
    // the OS default editor via the opener plugin, scoped to the app config
    // dir (see src-tauri/capabilities/default.json). `run:<cmd>` stays a
    // documented placeholder — running an arbitrary command is a security
    // decision left to the user, see odd/tasks/orbitbar-rebrand.md O9.
    const openConfig = async () => {
      try {
        const path = await invoke<string>("get_config_path");
        await openPath(path);
      } catch (err) {
        console.error("orbitbar: could not open config.json", err);
      }
    };

    // "Open pricing file" creates pricing.json from the built-in template
    // first if it does not exist yet (ensure_pricing_file), so the editor
    // never opens to a missing-file error.
    const openPricing = async () => {
      try {
        const path = await invoke<string>("ensure_pricing_file");
        await openPath(path);
      } catch (err) {
        console.error("orbitbar: could not open pricing.json", err);
      }
    };

    // Re-reads config.json and re-renders the bar in place — no restart, no
    // window resize beyond what the new collapsed/monitor/margin call for.
    // `cfg` is mutated in place (not replaced) so every closure that already
    // captured it — togglePanel, openUsagePanel, enableDrag, the context menu
    // actions — keeps seeing the fresh values without being rebound.
    const reloadConfig = async () => {
      try {
        const fresh = await invoke<ConfigPayload>("get_config");
        Object.assign(cfg, fresh.config);
        applyTheme(fresh.palette);
        themeName = fresh.palette.name;
        document.documentElement.style.setProperty("--ob-font-size", `${cfg.fontSize}px`);
        if (cells) {
          renderCells(cells, cfg.items);
          autostartCells = bindAutostartCells(cells, cfg.items);
          await refreshAutostartUi(autostartCells);
        }
        setCollapsedUi(cfg.collapsed);
        await applyState(cfg, panelOpen, false);
      } catch (err) {
        console.error("orbitbar: could not reload config", err);
      }
    };

    // Saves the choice, then reuses reloadConfig to fetch the resolved
    // palette and apply it live: same path as "Reload config", no restart.
    const setTheme = async (theme: ThemeName) => {
      cfg.theme = theme;
      await persistConfig(cfg);
      await reloadConfig();
    };

    const quit = async () => {
      try {
        await invoke("quit_app");
      } catch (err) {
        console.error("orbitbar: quit_app failed", err);
      }
    };

    const contextMenuActions = () =>
      buildContextMenuActions(
        cfg,
        { openConfig, openPricing, toggleCollapsed, reloadConfig, setTheme, quit },
        themeName,
      );


    const barEl = document.querySelector<HTMLElement>("#bar");
    if (barEl) {
      barEl.addEventListener("contextmenu", (ev) => {
        ev.preventDefault();
        void openContextMenu(
          cfg,
          (ev as MouseEvent).clientY,
          closePanel,
          setMenuOpen,
          contextMenuActions(),
          () => void closeContextMenu(cfg, setMenuOpen),
        );
      });
    }

    // Clicking anywhere outside the menu card closes it, same as Escape.
    window.addEventListener("pointerdown", (ev) => {
      if (!menuOpen || !contextMenuEl) return;
      const target = ev.target as HTMLElement;
      if (contextMenuEl.contains(target)) return;
      void closeContextMenu(cfg, setMenuOpen);
    });

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
          // Opening from closed starts on Today; switching agent while the
          // panel is already open keeps the user's current selection.
          if (!panelOpen) usageWindow = "today";
          activePanelAction = action;
          await openUsagePanel(action, body, togglePanel);
          return;
        }

        if (action === "edit-config") {
          activePanelAction = action;
          await openConfig();
          return;
        }

        // Other item actions (omniroute-status, run:cmd, ...) are wired up
        // separately; a cell click just demonstrates panel geometry until
        // they land.
        activePanelAction = action;
        await showActionPlaceholder(target.dataset.id ?? "", action, body, togglePanel);
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
      if (menuOpen) {
        void closeContextMenu(cfg, setMenuOpen);
        return;
      }
      void closePanel();
    });

    enableDrag(cfg);
  } catch (err) {
    console.error("orbitbar: failed to load config", err);
  }
});
