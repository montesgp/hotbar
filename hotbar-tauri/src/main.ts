import { invoke } from "@tauri-apps/api/core";

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
}

export interface Item {
  id: string;
  label: string;
  glyph: string;
  action: string;
  tooltip: string;
}

export interface HotbarConfig {
  monitor: string;
  margin: number;
  collapsed: boolean;
  theme: string;
  fontSize: number;
  items: Item[];
}

export interface ConfigPayload {
  config: HotbarConfig;
  palette: ThemePalette;
}

/** Map a Rust palette to CSS custom properties on :root. */
export function applyTheme(palette: ThemePalette): void {
  const root = document.documentElement;
  root.style.setProperty("--hb-bg", palette.background);
  root.style.setProperty("--hb-panel", palette.panel);
  root.style.setProperty("--hb-text", palette.text);
  root.style.setProperty("--hb-text-dim", palette.textDim);
  root.style.setProperty("--hb-hover-bg", palette.hoverBg);
  root.style.setProperty("--hb-hover-fg", palette.hoverFg);
  root.style.setProperty("--hb-radius-bar", `${palette.radiusBar}px`);
  root.style.setProperty("--hb-radius-cell", `${palette.radiusCell}px`);
}

export function renderCells(container: HTMLElement, items: Item[]): void {
  container.replaceChildren();
  for (const item of items) {
    const cell = document.createElement("div");
    cell.className = "cell";
    cell.title = item.tooltip;

    const glyph = document.createElement("span");
    glyph.className = "glyph";
    glyph.textContent = Number.parseInt(item.glyph, 16)
      ? String.fromCodePoint(Number.parseInt(item.glyph, 16))
      : item.glyph;
    cell.appendChild(glyph);

    const label = document.createElement("span");
    label.className = "label";
    label.textContent = item.label;
    cell.appendChild(label);

    container.appendChild(cell);
  }
}

window.addEventListener("DOMContentLoaded", async () => {
  try {
    const payload = await invoke<ConfigPayload>("get_config");
    applyTheme(payload.palette);
    document.documentElement.style.setProperty(
      "--hb-font-size",
      `${payload.config.fontSize}px`,
    );
    const cells = document.querySelector<HTMLElement>("#cells");
    if (cells) renderCells(cells, payload.config.items);
  } catch (err) {
    console.error("hotbar: failed to load config", err);
  }
});