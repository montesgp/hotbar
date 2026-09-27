//! Hotbar config v2 — schema, loader, and theme palettes as data.
//!
//! The config lives in the per-user app config dir (Tauri `app_config_dir`):
//!   Windows: %APPDATA%\com.hotbar.app\config.json
//!   Linux:   ~/.config/com.hotbar.app/config.json
//!   macOS:   ~/Library/Application Support/com.hotbar.app/config.json
//!
//! If the file does not exist on first launch it is created with the defaults
//! below, so end users always get an editable config without installing tools.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub monitor: String,
    pub margin: i32,
    pub collapsed: bool,
    pub theme: String,
    pub font_size: f64,
    pub items: Vec<Item>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            monitor: "primary".into(),
            margin: 8,
            collapsed: false,
            theme: "classic".into(),
            font_size: 10.0,
            items: default_items(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub label: String,
    pub glyph: String,
    pub action: String,
    pub tooltip: String,
}

/// Palette of visual tokens for one theme. Themes are data, never code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePalette {
    pub name: String,
    pub background: String,
    pub panel: String,
    pub text: String,
    pub text_dim: String,
    pub hover_bg: String,
    pub hover_fg: String,
    pub radius_bar: f64,
    pub radius_cell: f64,
}

const PALETTE_CLASSIC: &str = r##"{
  "name": "classic",
  "background": "#101014",
  "panel": "#3A3A44",
  "text": "#C8C8D4",
  "textDim": "#8A8A96",
  "hoverBg": "#3A3322",
  "hoverFg": "#E8C46A",
  "radiusBar": 22.0,
  "radiusCell": 8.0
}"##;

const PALETTE_DARK: &str = r##"{
  "name": "dark",
  "background": "#0D0D11",
  "panel": "#1F1F26",
  "text": "#E6E6F0",
  "textDim": "#9A9AA8",
  "hoverBg": "#2A2A35",
  "hoverFg": "#FFFFFF",
  "radiusBar": 12.0,
  "radiusCell": 6.0
}"##;

/// Resolve a theme name ("classic" | "dark" | any future name) to its palette.
/// Unknown names fall back to classic so a typo never breaks the bar.
pub fn palette_for(theme: &str) -> ThemePalette {
    let json = match theme {
        "dark" => PALETTE_DARK,
        _ => PALETTE_CLASSIC,
    };
    serde_json::from_str(json).expect("embedded palette must parse")
}

/// Load the config from the per-user dir, creating the file with defaults if
/// missing. A corrupt file fails loudly (a typo in config must be visible).
pub fn load(app: &AppHandle) -> Result<AppConfig, String> {
    let dir = config_dir(app)?;
    let path = dir.join("config.json");

    if !path.exists() {
        let cfg = AppConfig::default();
        persist(&path, &cfg)?;
        return Ok(cfg);
    }

    let raw = fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("invalid config.json: {e}"))
}

/// Persist a config back to the per-user dir (used by save-config and by the
/// first-launch default writer).
pub fn save(app: &AppHandle, cfg: &AppConfig) -> Result<(), String> {
    let dir = config_dir(app)?;
    let path = dir.join("config.json");
    persist(&path, cfg)
}

fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let resolver = app.path();
    let dir = resolver
        .app_config_dir()
        .map_err(|e| format!("cannot resolve app config dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

fn persist(path: &PathBuf, cfg: &AppConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| format!("cannot serialize config: {e}"))?;
    fs::write(path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn default_items() -> Vec<Item> {
    let tooltip = "none | omniroute-status | agent-usage | agent-usage:claude | agent-usage:codex | agent-usage:opencode | run:cmd | edit-config";
    vec![
        Item {
            id: "claude".into(),
            label: "claude".into(),
            glyph: "0x2733".into(),
            action: "agent-usage:claude".into(),
            tooltip: format!("Claude - historico del mes y por proyecto ({tooltip})"),
        },
        Item {
            id: "codex".into(),
            label: "codex".into(),
            glyph: "0x25CE".into(),
            action: "agent-usage:codex".into(),
            tooltip: format!("Codex - historico del mes y por proyecto ({tooltip})"),
        },
        Item {
            id: "opencode".into(),
            label: "opencode".into(),
            glyph: "0x25C8".into(),
            action: "agent-usage:opencode".into(),
            tooltip: format!("Opencode - historico del mes y por proyecto ({tooltip})"),
        },
        Item {
            id: "usage".into(),
            label: "uso".into(),
            glyph: "0x0024".into(),
            action: "agent-usage".into(),
            tooltip: "Uso de la sesion en vivo (claude/codex/opencode) - saldo en tiempo real".into(),
        },
        Item {
            id: "omniroute".into(),
            label: "omniroute".into(),
            glyph: "0x25A3".into(),
            action: "omniroute-status".into(),
            tooltip: "Estado del gateway OmniRoute (UP/DOWN + combos)".into(),
        },
        Item {
            id: "settings".into(),
            label: "ajustes".into(),
            glyph: "0x2699".into(),
            action: "edit-config".into(),
            tooltip: "Abrir hotbar/config.json".into(),
        },
    ]
}