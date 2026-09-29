import { invoke } from "@tauri-apps/api/core";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import {
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
  /** Demonstration cell: shown only while `showExamples` is on. */
  example?: boolean;
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
  /** Whether items flagged `example` are visible. Off by default. */
  showExamples: boolean;
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
const CONTEXT_MENU_MIN_HEIGHT = 337;
/** Nine entries plus three separators, sized from `.context-menu`'s own CSS;
 * kept as a constant instead of measured so opening the menu never needs an
 * extra hidden-then-remeasure paint. */
const CONTEXT_MENU_HEIGHT_ESTIMATE = 310;

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
  // Pressed = the hover wash pulled toward the hover foreground. It must stay
  // clearly different from the foreground itself, or the glyph disappears
  // while the button is held.
  root.style.setProperty(
    "--ob-press-bg",
    `color-mix(in srgb, ${palette.hoverBg} 78%, ${palette.hoverFg})`,
  );
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
 *
 * Example items are always rendered but stay `hidden` until `showExamples` is
 * on, so toggling them (`applyExamplesVisibility`) is a per-cell attribute
 * change, not a rebuild.
 */
export function renderCells(container: HTMLElement, items: Item[], showExamples = false): void {
  container.replaceChildren(buildCells(items, showExamples));
}

/** Builds the cells off-DOM, so a caller can swap them in with one call. */
export function buildCells(items: Item[], showExamples: boolean): DocumentFragment {
  const fragment = document.createDocumentFragment();
  for (const item of items) {
    const cell = document.createElement("div");
    cell.className = "cell";
    cell.title = item.tooltip;
    cell.setAttribute("role", "button");
    cell.setAttribute("aria-label", item.tooltip || item.label);
    cell.dataset.id = item.id;
    cell.dataset.action = item.action;
    if (item.example) {
      cell.dataset.example = "true";
      cell.hidden = !showExamples;
    }
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

    fragment.appendChild(cell);
  }
  return fragment;
}

/** Shows or hides every example cell in place. */
function applyExamplesVisibility(container: HTMLElement, show: boolean): void {
  for (const cell of container.querySelectorAll<HTMLElement>('.cell[data-example="true"]')) {
    cell.hidden = !show;
  }
}

/**
 * Right-center a physical window of `size` on a monitor, honoring the config
 * margin. The Rust `snap_window` command computes the target position and
 * applies it together with the size in ONE native operation: separate
 * set_size / set_position calls show an in-between frame (new size, old
 * position) that makes the right-anchored bar visibly jump. It also toggles
 * `resizable` around the change, because the window is created
 * `resizable: false` and the toolkit (tao) then locks min/max size to the size
 * at that moment, silently clamping every later resize back to it.
 */
async function snapToMonitor(
  monitor: Monitor,
  size: PhysicalSize,
  margin: number,
): Promise<void> {
  // Re-snapping to the geometry the window already has still makes the OS
  // repaint it, so an unchanged target is skipped. A drag clears the key
  // (`enableDrag`), because the user moved the window since the last snap.
  const key = [
    monitor.position.x,
    monitor.position.y,
    monitor.size.width,
    monitor.size.height,
    size.width,
    size.height,
    margin,
  ].join(",");
  if (key === lastSnapKey) return;
  await invoke("snap_window", {
    target: {
      monitorX: monitor.position.x,
      monitorY: monitor.position.y,
      monitorWidth: monitor.size.width,
      monitorHeight: monitor.size.height,
      width: size.width,
      height: size.height,
      margin,
    },
  });
  lastSnapKey = key;
}

/** Geometry of the last successful `snap_window`, or null when unknown. */
let lastSnapKey: string | null = null;

/** Resize + reposition the window for the requested collapse/panel state. */
async function applyState(
  cfg: OrbitbarConfig,
  panelOpen: boolean,
  menuOpen: boolean,
): Promise<void> {
  const size = sizeFor(cfg.collapsed, panelOpen, menuOpen);
  const monitor = await currentMonitor();
  if (!monitor) return;
  await snapToMonitor(monitor, size, cfg.margin);
}

/** One clickable row, or the literal string "separator" for a divider. */
interface ContextMenuAction {
  label: string;
  run: () => void | Promise<void>;
  /** Present only on checkable entries: true marks the choice in effect. */
  checked?: boolean;
  /** How a checkable entry behaves: "radio" (one of a group, the default) or
   * an independent "checkbox". */
  kind?: "radio" | "checkbox";
  /** True when the action resizes the window itself, so closing the menu
   * must not shrink it first (that would be a second, visible resize). */
  resizesWindow?: boolean;
}

/**
 * The fixed entry list, in order. "Collapse"/"Expand" reflects `cfg.collapsed`
 * so the label always matches what the click will actually do, and the theme
 * choice in effect (`currentTheme`, the resolved palette name) carries a check,
 * and so does "Start with system" when `autostartOn` (the real OS registration).
 */
function buildContextMenuActions(
  cfg: OrbitbarConfig,
  handlers: {
    openConfig: () => void | Promise<void>;
    openPricing: () => void | Promise<void>;
    toggleCollapsed: () => void | Promise<void>;
    reloadConfig: () => void | Promise<void>;
    setTheme: (theme: ThemeName) => void | Promise<void>;
    toggleAutostart: () => void | Promise<void>;
    toggleExamples: () => void | Promise<void>;
    quit: () => void | Promise<void>;
  },
  currentTheme: string,
  autostartOn: boolean,
): (ContextMenuAction | "separator")[] {
  return [
    { label: "Edit config", run: handlers.openConfig },
    { label: "Open pricing file", run: handlers.openPricing },
    {
      label: cfg.collapsed ? "Expand" : "Collapse",
      run: handlers.toggleCollapsed,
      resizesWindow: true,
    },
    { label: "Reload config", run: handlers.reloadConfig },
    "separator",
    { label: "Light", checked: currentTheme === "light", run: () => handlers.setTheme("light") },
    { label: "Dark", checked: currentTheme === "dark", run: () => handlers.setTheme("dark") },
    "separator",
    {
      label: "Start with system",
      checked: autostartOn,
      kind: "checkbox",
      run: handlers.toggleAutostart,
    },
    {
      label: "Show example action",
      checked: cfg.showExamples,
      kind: "checkbox",
      run: handlers.toggleExamples,
    },
    "separator",
    { label: "Quit Orbitbar", run: handlers.quit },
  ];
}

/** Paints the menu card. The first entry clicked wins: the card is marked
 * `.chosen` (no pointer events) and later clicks are ignored. `onSelect` is
 * awaited before the action runs, so the menu is always fully closed (faded out, window resized back down unless the
 * action resizes it itself) first and no action that re-renders or resizes
 * races the close, whether the action succeeds or fails. */
function renderContextMenu(
  el: HTMLElement,
  actions: (ContextMenuAction | "separator")[],
  onSelect: (resize: boolean) => Promise<void>,
): void {
  el.replaceChildren();
  el.classList.remove("chosen");
  // One action per menu session: the card stays in the DOM while it fades out,
  // so a second click must not run a second entry.
  let chosen = false;
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
      // Checkable entry: the check slot is always present so labels stay
      // aligned whether or not this one is the current choice.
      btn.setAttribute("role", action.kind === "checkbox" ? "menuitemcheckbox" : "menuitemradio");
      btn.setAttribute("aria-checked", String(action.checked));
      const mark = document.createElement("span");
      mark.className = "context-menu-check";
      mark.textContent = action.checked ? "✓" : "";
      btn.append(mark, action.label);
    }
    btn.addEventListener("click", () => {
      if (chosen) return;
      chosen = true;
      el.classList.add("chosen");
      void (async () => {
        try {
          await onSelect(!action.resizesWindow);
        } finally {
          await action.run();
        }
      })();
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

/** Shows one line of text in the panel body and opens the panel. */
async function showPanelMessage(
  text: string,
  body: HTMLElement,
  togglePanel: (open: boolean) => Promise<void>,
): Promise<void> {
  body.replaceChildren();
  const line = document.createElement("div");
  line.textContent = text;
  body.appendChild(line);
  await togglePanel(true);
}

/** The shared fallback for an item action with no behavior (`omniroute-status`
 * and unknown names): show which id/action fired inside the panel instead of
 * doing nothing. */
async function showActionPlaceholder(
  id: string,
  action: string,
  body: HTMLElement,
  togglePanel: (open: boolean) => Promise<void>,
): Promise<void> {
  await showPanelMessage(`${id}: ${action}`, body, togglePanel);
}

/** True for an absolute http:// or https:// URL, the only kinds `open:` accepts.
 * The opener capability enforces the same scope on the Rust side. */
function isWebUrl(raw: string): boolean {
  try {
    const { protocol } = new URL(raw);
    return protocol === "http:" || protocol === "https:";
  } catch {
    return false;
  }
}

/** Runs a launch action (`open:<url>` or `run:<program> [args]`). A `run:` sends
 * only the item id: the backend looks the command up in its own config.
 * Returns an error message for the panel, or null on success. */
async function runLaunchAction(id: string, action: string): Promise<string | null> {
  try {
    if (action.startsWith("open:")) {
      const url = action.slice("open:".length).trim();
      if (!isWebUrl(url)) return `open: accepts only http:// and https:// URLs (got "${url}")`;
      await openUrl(url);
    } else {
      await invoke("run_command", { id });
    }
    return null;
  } catch (err) {
    console.error("orbitbar: launch action failed", action, err);
    return String(err);
  }
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
  onSelect: (resize: boolean) => Promise<void>,
): Promise<void> {
  const menu = contextMenuEl;
  if (!menu) return;
  // A menu still fading out must finish (and shrink the window) before the
  // next one grows it.
  if (menuClosing) await menuClosing;
  await closePanel();
  setMenuOpen(true);
  renderContextMenu(menu, actions, onSelect);
  // Grow the window while the card is still hidden, then fade it in.
  await applyState(cfg, false, true);
  const size = sizeFor(cfg.collapsed, false, true);
  positionContextMenu(menu, clickY, size.height);
  menu.hidden = false;
  // Force a style flush so the transition starts from the hidden state.
  void menu.offsetWidth;
  menu.classList.add("open");
}

/** Fade duration of the menu card; keep in step with `.context-menu` in
 * styles.css. */
const MENU_FADE_MS = 150;

/** The close in progress, so overlapping closes (Escape, outside click, a
 * collapse handler) share one fade and one resize. */
let menuClosing: Promise<void> | null = null;

/** Fades the card out and resolves once the transition is over (or at once
 * when the user prefers reduced motion). */
function fadeOutMenu(menu: HTMLElement): Promise<void> {
  menu.classList.remove("open");
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    return Promise.resolve();
  }
  return new Promise((resolve) => {
    const finish = () => {
      menu.removeEventListener("transitionend", onEnd);
      clearTimeout(timer);
      resolve();
    };
    const onEnd = (ev: TransitionEvent) => {
      if (ev.target === menu && ev.propertyName === "opacity") finish();
    };
    menu.addEventListener("transitionend", onEnd);
    // Safety net if the transition never fires (e.g. the window is hidden).
    const timer = setTimeout(finish, MENU_FADE_MS + 60);
  });
}

/** Closes the menu: fade the card out first, only THEN hide it and shrink the
 * window, so the shrink never shows a half-drawn card. `resize: false` is for
 * actions that resize the window themselves (collapse/expand). Restores the
 * plain collapsed/expanded size, never the panel. */
function closeContextMenu(
  cfg: OrbitbarConfig,
  setMenuOpen: (open: boolean) => void,
  resize = true,
): Promise<void> {
  const menu = contextMenuEl;
  if (menuClosing) return menuClosing;
  if (!menu || menu.hidden) return Promise.resolve();
  setMenuOpen(false);
  menuClosing = (async () => {
    await fadeOutMenu(menu);
    menu.hidden = true;
    if (resize) await applyState(cfg, false, false);
  })().finally(() => {
    menuClosing = null;
  });
  return menuClosing;
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
    lastSnapKey = null;
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
function bindAutostartCells(cells: ParentNode, items: Item[]): Map<HTMLElement, string> {
  const map = new Map<HTMLElement, string>();
  for (const item of items) {
    if (item.action !== "toggle-autostart") continue;
    const cell = cells.querySelector<HTMLElement>(`.cell[data-id="${CSS.escape(item.id)}"]`);
    if (cell) map.set(cell, item.tooltip);
  }
  return map;
}

/** Applies one autostart state to every autostart cell, in place. */
function paintAutostart(autostartCells: Map<HTMLElement, string>, on: boolean): void {
  for (const [cell, tooltip] of autostartCells) {
    applyAutostartUi(cell, tooltip, on);
  }
}

/** The real OS registration, not the config: they can disagree, since the user
 * can revoke the Run key in OS settings without touching our config. `null`
 * when it cannot be read; callers pick their own fallback. */
async function readAutostart(): Promise<boolean | null> {
  try {
    return await isAutostartEnabled();
  } catch (err) {
    console.error("orbitbar: could not read autostart state", err);
    return null;
  }
}

/** Repaints the autostart cells from the real OS registration. */
async function refreshAutostartUi(autostartCells: Map<HTMLElement, string>): Promise<void> {
  if (autostartCells.size === 0) return;
  const on = await readAutostart();
  if (on !== null) paintAutostart(autostartCells, on);
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
    if (cells) renderCells(cells, cfg.items, cfg.showExamples);

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
      // The close skips its own shrink: the single applyState below lands the
      // window straight on its final size. The new look is painted first, in
      // the still-large (right-anchored) window, so the resize hides nothing.
      await closeContextMenu(cfg, setMenuOpen, false);
      if (cfg.collapsed) {
        cfg.collapsed = false;
        setCollapsedUi(false);
        await applyState(cfg, panelOpen, false);
      } else {
        cfg.collapsed = true;
        setCollapsedUi(true);
        await togglePanel(false);
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

    // The menu's "Edit config" entry (reached from the ⚙ cell or a right
    // click) opens config.json in the OS default editor via the opener
    // plugin, scoped to the app config dir (see
    // src-tauri/capabilities/default.json). Launch actions (`open:<url>`,
    // `run:<program> [args]`) act only on an explicit cell click.
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
    // actions — keeps seeing the fresh values without being rebound. The new
    // cells are built (and their autostart state painted) off-DOM, then swapped
    // in with one call, so no frame shows a blank or half-updated bar.
    const reloadConfig = async () => {
      try {
        const fresh = await invoke<ConfigPayload>("get_config");
        const autostartOn = (await readAutostart()) ?? fresh.config.autoStart;
        Object.assign(cfg, fresh.config);
        applyTheme(fresh.palette);
        themeName = fresh.palette.name;
        document.documentElement.style.setProperty("--ob-font-size", `${cfg.fontSize}px`);
        if (cells) {
          const fragment = buildCells(cfg.items, cfg.showExamples);
          const bound = bindAutostartCells(fragment, cfg.items);
          paintAutostart(bound, autostartOn);
          cells.replaceChildren(fragment);
          autostartCells = bound;
        }
        setCollapsedUi(cfg.collapsed);
        await applyState(cfg, panelOpen, false);
      } catch (err) {
        console.error("orbitbar: could not reload config", err);
      }
    };

    // Saves the choice, then fetches the resolved palette and applies it: only
    // CSS custom properties change. The cells are not rebuilt.
    const setTheme = async (theme: ThemeName) => {
      cfg.theme = theme;
      await persistConfig(cfg);
      try {
        const fresh = await invoke<ConfigPayload>("get_config");
        applyTheme(fresh.palette);
        themeName = fresh.palette.name;
      } catch (err) {
        console.error("orbitbar: could not apply theme", err);
      }
    };

    // Flips the OS registration and persists the choice. Shared by the
    // autostart cell and the menu's "Start with system" entry. The direction
    // comes from the real OS state (the user may have revoked the entry
    // outside the app), falling back to the config when it cannot be read.
    // If the OS refuses the write, neither the config nor the UI changes.
    const toggleAutostart = async () => {
      const next = !((await readAutostart()) ?? cfg.autoStart);
      try {
        if (next) await enableAutostart();
        else await disableAutostart();
      } catch (err) {
        console.error("orbitbar: autostart could not be changed", err);
        await refreshAutostartUi(autostartCells);
        return;
      }
      cfg.autoStart = next;
      paintAutostart(autostartCells, next);
      await persistConfig(cfg);
    };

    // Shows or hides the example cells in place and persists the choice.
    const toggleExamples = async () => {
      cfg.showExamples = !cfg.showExamples;
      if (cells) applyExamplesVisibility(cells, cfg.showExamples);
      await persistConfig(cfg);
    };

    const quit = async () => {
      try {
        await invoke("quit_app");
      } catch (err) {
        console.error("orbitbar: quit_app failed", err);
      }
    };

    const contextMenuActions = async () =>
      buildContextMenuActions(
        cfg,
        { openConfig, openPricing, toggleCollapsed, reloadConfig, setTheme, toggleAutostart, toggleExamples, quit },
        themeName,
        (await readAutostart()) ?? cfg.autoStart,
      );

    // Shared by right-click on the bar and left-click on the settings cell,
    // so both open the very same menu anchored at the pointer.
    const showContextMenu = async (clientY: number) =>
      openContextMenu(
        cfg,
        clientY,
        closePanel,
        setMenuOpen,
        await contextMenuActions(),
        (resize) => closeContextMenu(cfg, setMenuOpen, resize),
      );

    const barEl = document.querySelector<HTMLElement>("#bar");
    if (barEl) {
      barEl.addEventListener("contextmenu", (ev) => {
        ev.preventDefault();
        void showContextMenu((ev as MouseEvent).clientY);
      });
    }

    // Clicking anywhere outside the menu card closes it, same as Escape.
    window.addEventListener("pointerdown", (ev) => {
      if (!menuOpen || !contextMenuEl) return;
      const target = ev.target as HTMLElement;
      if (contextMenuEl.contains(target)) return;
      // The settings cell toggles the menu in its own click handler; closing
      // here first would make that click see a closed menu and reopen it.
      if (target.closest('.cell[data-action="edit-config"]')) return;
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
          await toggleAutostart();
          return;
        }

        const action = target.dataset.action ?? "";

        // The settings cell opens the context menu (where "Edit config"
        // lives) instead of acting directly. It never becomes the panel's
        // active action: the menu closes any open panel first.
        if (action === "edit-config") {
          if (menuOpen) {
            await closeContextMenu(cfg, setMenuOpen);
            return;
          }
          await showContextMenu((ev as MouseEvent).clientY);
          return;
        }

        // Clicking the cell that is already driving the open panel closes
        // it, same as Escape or the close button.
        if (panelOpen && activePanelAction === action) {
          await closePanel();
          return;
        }

        const body = document.querySelector<HTMLElement>("#panel-body");
        if (!body) return;

        // Launch actions act in place: on success the panel is left as it is,
        // on failure the reason is shown in the panel like any action error.
        if (action.startsWith("open:") || action.startsWith("run:")) {
          const failure = await runLaunchAction(target.dataset.id ?? "", action);
          if (failure !== null) {
            activePanelAction = action;
            await showPanelMessage(failure, body, togglePanel);
          }
          return;
        }

        if (isUsageAction(action)) {
          // Opening from closed starts on Today; switching agent while the
          // panel is already open keeps the user's current selection.
          if (!panelOpen) usageWindow = "today";
          activePanelAction = action;
          await openUsagePanel(action, body, togglePanel);
          return;
        }

        // Other item actions (omniroute-status, ...) are wired up separately;
        // a cell click just demonstrates panel geometry until they land.
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
