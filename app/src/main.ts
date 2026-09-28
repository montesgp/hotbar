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

/** Resize + reposition the window for the requested collapse/panel state. */
async function applyState(
  cfg: OrbitbarConfig,
  panelOpen: boolean,
): Promise<void> {
  const size = sizeFor(cfg.collapsed, panelOpen);
  const monitor = await currentMonitor();
  if (!monitor) return;
  await win.setSize(size);
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
    setCollapsedUi(cfg.collapsed);
    setPanelUi(panelOpen);

    const togglePanel = async (open: boolean) => {
      panelOpen = open;
      setPanelUi(panelOpen);
      await applyState(cfg, panelOpen);
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

    // Panel wiring for item actions lands with the readers (HB22/HB23); until
    // then a cell click only demonstrates the 392px panel geometry.
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

        const body = document.querySelector<HTMLElement>("#panel-body");
        if (body) {
          body.replaceChildren();
          const line = document.createElement("div");
          line.textContent = `${target.dataset.id}: ${target.dataset.action}`;
          body.appendChild(line);
        }
        await togglePanel(true);
      });
    }

    enableDrag(cfg);
  } catch (err) {
    console.error("orbitbar: failed to load config", err);
  }
});
