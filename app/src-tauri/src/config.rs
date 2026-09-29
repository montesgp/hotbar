//! Orbitbar config v2 — schema, loader, and theme palettes as data.
//!
//! The config lives in the per-user app config dir (Tauri `app_config_dir`):
//!   Windows: %APPDATA%\com.orbitbar.app\config.json
//!   Linux:   ~/.config/com.orbitbar.app/config.json
//!   macOS:   ~/Library/Application Support/com.orbitbar.app/config.json
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
    /// Whether the app should be registered to launch at login. The config is
    /// the source of truth: lib.rs reconciles the OS entry against it on every
    /// start. Defaults to true, and because AppConfig carries a container-level
    /// serde default, a config written before this field existed simply gets
    /// true rather than failing to parse.
    pub auto_start: bool,
    /// Whether items flagged `example` are shown. Off by default: examples
    /// only demonstrate what a cell can do, they are not part of the user's bar.
    pub show_examples: bool,
    /// Where the bar was last dropped: its top-left in physical screen pixels
    /// (negative on a monitor left of or above the primary). Absent until the
    /// bar is first moved, and ignored when it no longer lies on any monitor;
    /// the bar then goes right-center of `monitor` with `margin`, as before.
    /// Omitted from config.json while unset, so an untouched file stays clean.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    pub items: Vec<Item>,
}

/// A point in physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            monitor: "primary".into(),
            margin: 8,
            collapsed: false,
            theme: "dark".into(),
            font_size: 10.0,
            auto_start: true,
            show_examples: false,
            position: None,
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
    /// Marks a demonstration cell: rendered only while `showExamples` is on.
    /// Omitted from config.json when false, so the file stays clean.
    #[serde(default, skip_serializing_if = "is_false")]
    pub example: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Palette of visual tokens for one theme. Themes are data, never code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
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
    pub radius_handle: f64,
    pub tab_radius: f64,
    pub bar_border: String,
    pub gradient_top: String,
    pub gradient_bottom: String,
    pub half_moon: bool,
    pub moon_radius: f64,
}

/// Written out field by field on purpose: `#[serde(default)]` calls this for
/// any key a palette omits, so building it from `PALETTE_DARK` would recurse
/// through the deserializer forever. The `the_default_theme_is_dark` test
/// keeps it in step with `PALETTE_DARK`.
impl Default for ThemePalette {
    fn default() -> Self {
        Self {
            name: "dark".into(),
            background: "#101014".into(),
            panel: "#3A3A44".into(),
            text: "#C8C8D4".into(),
            text_dim: "#8A8A96".into(),
            hover_bg: "#3A3322".into(),
            hover_fg: "#E8C46A".into(),
            radius_bar: 22.0,
            radius_cell: 22.0,
            radius_handle: 8.0,
            tab_radius: 23.0,
            bar_border: "#3A3A44".into(),
            gradient_top: "#232329".into(),
            gradient_bottom: "#101014".into(),
            half_moon: true,
            moon_radius: 200.0,
        }
    }
}

/// The original look (formerly named `classic`): the crescent bar in a dark
/// palette with a gold hover accent.
const PALETTE_DARK: &str = r##"{
  "name": "dark",
  "background": "#101014",
  "panel": "#3A3A44",
  "text": "#C8C8D4",
  "textDim": "#8A8A96",
  "hoverBg": "#3A3322",
  "hoverFg": "#E8C46A",
  "radiusBar": 22.0,
  "radiusCell": 22.0,
  "radiusHandle": 8.0,
  "tabRadius": 23.0,
  "barBorder": "#3A3A44",
  "gradientTop": "#232329",
  "gradientBottom": "#101014",
  "halfMoon": true,
  "moonRadius": 200.0
}"##;

/// Same crescent as `PALETTE_DARK`: every shape token is identical and only
/// the colors change. Text and dim text keep at least 4.5:1 contrast on the
/// panel, and the hover accent is a dark amber on a light amber wash.
const PALETTE_LIGHT: &str = r##"{
  "name": "light",
  "background": "#F4F4F8",
  "panel": "#F0F0F5",
  "text": "#22222B",
  "textDim": "#62626F",
  "hoverBg": "#F0E0B0",
  "hoverFg": "#8A5A00",
  "radiusBar": 22.0,
  "radiusCell": 22.0,
  "radiusHandle": 8.0,
  "tabRadius": 23.0,
  "barBorder": "#C9C9D6",
  "gradientTop": "#FFFFFF",
  "gradientBottom": "#E6E6EE",
  "halfMoon": true,
  "moonRadius": 200.0
}"##;

/// Resolve a theme name ("light" | "dark") to its palette. Anything else,
/// including the retired name "classic" and typos, resolves to dark so an old
/// or hand-edited config never breaks the bar.
pub fn palette_for(theme: &str) -> ThemePalette {
    let json = match theme {
        "light" => PALETTE_LIGHT,
        _ => PALETTE_DARK,
    };
    serde_json::from_str(json).expect("embedded palette must parse")
}

/// Load the config from the per-user dir, creating the file with defaults if
/// missing.
///
/// A hand-edited config that fails to parse must NOT brick the widget. The
/// product invites users to edit this file (it is the extension point), and a
/// single trailing comma should not cost them their bar. So a broken file is
/// quarantined next to itself and the app starts on defaults; the recovery is
/// reported on stderr instead of swallowed.
pub fn load(app: &AppHandle) -> Result<AppConfig, String> {
    let dir = config_dir(app)?;
    let path = dir.join("config.json");

    if !path.exists() {
        let cfg = AppConfig::default();
        persist(&path, &cfg)?;
        return Ok(cfg);
    }

    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;

    match serde_json::from_str::<AppConfig>(strip_bom(&raw)) {
        Ok(cfg) => Ok(cfg),
        Err(err) => {
            let backup = path.with_extension("json.invalid");
            let _ = fs::rename(&path, &backup);
            let cfg = AppConfig::default();
            persist(&path, &cfg)?;
            eprintln!(
                "orbitbar: config.json was invalid ({err}); moved to {} and started on defaults",
                backup.display()
            );
            Ok(cfg)
        }
    }
}

/// Windows editors (Notepad, PowerShell 5.1) write a UTF-8 BOM, which
/// serde_json rejects at byte 0 with "expected value at line 1 column 1".
/// Strip it so a BOM-edited config loads like any other.
fn strip_bom(raw: &str) -> &str {
    raw.strip_prefix('\u{feff}').unwrap_or(raw)
}

/// Persist a config back to the per-user dir (used by save-config and by the
/// first-launch default writer).
pub fn save(app: &AppHandle, cfg: &AppConfig) -> Result<(), String> {
    let dir = config_dir(app)?;
    let path = dir.join("config.json");
    persist(&path, cfg)
}

/// Resolves the per-user app config dir, creating it if missing. `pub` so
/// callers outside this module (the `get_usage` command, to find
/// `pricing.json` next to `config.json`) can reuse the exact same directory
/// resolution instead of re-deriving it.
pub fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
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

/// Default content written for a first-time `pricing.json`: the same
/// explanatory `_readme` as `app/pricing.example.json`, but with an empty
/// override table instead of the example's `my-local-model` row, so the file
/// carries no sample data that could be mistaken for something real.
const PRICING_TEMPLATE: &str = r##"{
  "_readme": [
    "Add or override model prices here. get_usage reloads this file on every",
    "call, so edits apply on the next refresh without restarting the app.",
    "",
    "Key under 'models' = a model id, or a prefix of one (e.g. 'claude-opus-5-5'",
    "also matches 'claude-opus-5-5-20260926'). An entry here wins over a",
    "built-in row with the exact same key; a longer key always wins over a",
    "shorter one, built-in or override. Amounts are USD per 1M tokens.",
    "",
    "cacheRead / cacheWrite5m / cacheWrite1h are optional. When omitted they",
    "default to 0.1x, 1.25x and 2x that entry's own input price - Anthropic's",
    "published cache multipliers."
  ],
  "models": {}
}
"##;

/// Creates `pricing.json` at `path` from `PRICING_TEMPLATE` if it does not
/// exist yet, so "Open pricing file" always opens something useful instead
/// of the OS reporting a missing file. Never touches an existing file, even
/// an empty or malformed one - once the user has a pricing.json, it is
/// theirs to edit, not ours to regenerate.
pub fn ensure_pricing_file(path: &PathBuf) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    fs::write(path, PRICING_TEMPLATE).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn default_items() -> Vec<Item> {
    let tooltip = "none | agent-usage | agent-usage:claude | agent-usage:codex | agent-usage:opencode | open:<url> | run:<program> [args] | panel:<program> [args] | edit-config";
    vec![
        Item {
            id: "claude".into(),
            label: "claude".into(),
            glyph: "0x2733".into(),
            action: "agent-usage:claude".into(),
            tooltip: format!("Claude - month history and per-project ({tooltip})"),
            example: false,
        },
        Item {
            id: "codex".into(),
            label: "codex".into(),
            glyph: "0x25CE".into(),
            action: "agent-usage:codex".into(),
            tooltip: format!("Codex - month history and per-project ({tooltip})"),
            example: false,
        },
        Item {
            id: "opencode".into(),
            label: "opencode".into(),
            glyph: "0x25C8".into(),
            action: "agent-usage:opencode".into(),
            tooltip: format!("Opencode - month history and per-project ({tooltip})"),
            example: false,
        },
        Item {
            id: "usage".into(),
            label: "usage".into(),
            glyph: "0x0024".into(),
            action: "agent-usage".into(),
            tooltip: "Live session usage (claude/codex/opencode) - real-time balance".into(),
            example: false,
        },
        Item {
            id: "github".into(),
            label: "github".into(),
            // U+2197 NORTH EAST ARROW: "opens outside the app".
            glyph: "0x2197".into(),
            action: "open:https://github.com/montesgp/orbitbar".into(),
            tooltip: "Example action - opens Orbitbar on GitHub".into(),
            example: true,
        },
        Item {
            id: "settings".into(),
            label: "settings".into(),
            glyph: "0x2699".into(),
            action: "edit-config".into(),
            tooltip: "Orbitbar menu - edit config, theme, quit".into(),
            example: false,
        },
        Item {
            id: "autostart".into(),
            label: "autostart".into(),
            // U+23FB POWER SYMBOL. The state is carried by dimming and the
            // tooltip, not by a second glyph, so the icon stays the user's.
            glyph: "0x23FB".into(),
            action: "toggle-autostart".into(),
            tooltip: "Launch Orbitbar at login".into(),
            example: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every embedded palette must parse. This is the one thing a typo in a
    /// const string would break at runtime on the user's first launch, and it
    /// is invisible until the window paints nothing.
    #[test]
    fn every_embedded_palette_parses() {
        for theme in ["light", "dark"] {
            let p = palette_for(theme);
            assert_eq!(p.name, theme);
            assert!(!p.background.is_empty());
            assert!(p.radius_cell > 0.0);
        }
    }

    /// Only `light` and `dark` exist. The retired name `classic`, a typo and
    /// an empty string all resolve to dark, so an old config.json keeps its
    /// look and a typo never breaks the bar.
    #[test]
    fn classic_and_unknown_themes_resolve_to_dark() {
        for name in ["classic", "clasci", ""] {
            assert_eq!(palette_for(name).name, "dark", "theme {name:?}");
        }
    }

    /// Light hover wash: a slightly stronger amber than the first cut
    /// (`#F2E6C4`), still at least 4.5:1 against the hover foreground.
    #[test]
    fn light_hover_uses_a_stronger_amber_wash() {
        let p = palette_for("light");
        assert_eq!(p.hover_bg, "#F0E0B0");
        assert_eq!(p.hover_fg, "#8A5A00");
    }

    #[test]
    fn the_default_theme_is_dark() {
        assert_eq!(AppConfig::default().theme, "dark");
        let (default, dark) = (ThemePalette::default(), palette_for("dark"));
        assert_eq!(
            serde_json::to_value(&default).unwrap(),
            serde_json::to_value(&dark).unwrap(),
            "Default must stay identical to PALETTE_DARK"
        );
    }

    /// `dark` is the former `classic` palette, value for value: users who had
    /// `classic` must see no visual change.
    #[test]
    fn dark_keeps_the_former_classic_colors() {
        let p = palette_for("dark");
        assert_eq!(p.background, "#101014");
        assert_eq!(p.panel, "#3A3A44");
        assert_eq!(p.text, "#C8C8D4");
        assert_eq!(p.text_dim, "#8A8A96");
        assert_eq!(p.hover_bg, "#3A3322");
        assert_eq!(p.hover_fg, "#E8C46A");
        assert_eq!(p.bar_border, "#3A3A44");
        assert_eq!(p.gradient_top, "#232329");
        assert_eq!(p.gradient_bottom, "#101014");
    }

    /// The themes differ only in color: the crescent shape must be identical.
    #[test]
    fn light_shares_every_shape_token_with_dark() {
        let (light, dark) = (palette_for("light"), palette_for("dark"));
        assert_eq!(light.half_moon, dark.half_moon);
        assert_eq!(light.moon_radius, dark.moon_radius);
        assert_eq!(light.radius_bar, dark.radius_bar);
        assert_eq!(light.radius_cell, dark.radius_cell);
        assert_eq!(light.radius_handle, dark.radius_handle);
        assert_eq!(light.tab_radius, dark.tab_radius);
        assert_ne!(light.background, dark.background);
        assert_ne!(light.text, dark.text);
    }

    /// The crescent only works because the moon radius matches the bar height;
    /// a regression here silently turns the bar into a rounded rectangle.
    #[test]
    fn moon_geometry_matches_the_bar_in_every_theme() {
        for theme in ["light", "dark"] {
            let p = palette_for(theme);
            assert!(p.half_moon, "{theme}");
            assert_eq!(p.moon_radius, 200.0, "moon radius is half the 400px bar height");
            assert_eq!(p.tab_radius, 23.0, "collapsed tab is 46x46, so half is 23");
        }
    }

    /// Cells are 44px circles in the WPF original, not 8px rounded squares.
    #[test]
    fn cell_radius_is_a_circle_not_a_rounded_square() {
        for theme in ["light", "dark"] {
            assert_eq!(palette_for(theme).radius_cell, 22.0);
        }
    }

    /// A BOM-edited config is a normal thing on Windows: Notepad and
    /// PowerShell 5.1 both add one, and serde_json rejects it at byte 0.
    #[test]
    fn strip_bom_lets_a_bom_edited_config_parse() {
        let clean = r#"{"monitor":"primary","margin":8,"collapsed":false,"theme":"dark","fontSize":10.0,"items":[]}"#;
        let bommed = format!("\u{feff}{clean}");
        assert!(serde_json::from_str::<AppConfig>(clean).is_ok());
        assert!(
            serde_json::from_str::<AppConfig>(&bommed).is_err(),
            "precondition: serde_json must reject the BOM"
        );
        assert!(serde_json::from_str::<AppConfig>(strip_bom(&bommed)).is_ok());
    }

    /// A config missing optional keys must still load, so an older file written
    /// by a previous version is not treated as corrupt.
    #[test]
    fn partial_config_gets_defaults() {
        let cfg: AppConfig = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(cfg.theme, "dark");
        assert_eq!(cfg.monitor, "primary");
        assert!(!cfg.items.is_empty(), "items must fall back to the defaults");
    }

    /// A config written before `autoStart` existed must migrate to true rather
    /// than fail to parse, otherwise upgrading the app would silently drop the
    /// user's whole setup. This is the case that makes the container-level
    /// serde default load-bearing.
    #[test]
    fn a_config_without_autostart_migrates_to_true() {
        let cfg: AppConfig = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert!(
            cfg.auto_start,
            "an older config must opt in to autostart, not out of it"
        );
    }

    /// An explicit false has to win over the default, otherwise the toggle
    /// could never stick.
    #[test]
    fn an_explicit_auto_start_false_is_preserved() {
        let cfg: AppConfig = serde_json::from_str(r#"{"autoStart":false}"#).unwrap();
        assert!(!cfg.auto_start);
    }

    /// The usage panel always opens on Today and the selector is never
    /// persisted, so `usageWindow` is not part of the schema any more.
    #[test]
    fn usage_window_is_not_persisted() {
        let json = serde_json::to_string(&AppConfig::default()).unwrap();
        assert!(!json.contains("usageWindow"), "selector must not be written: {json}");
    }

    /// A config.json written by an older build still carries `usageWindow`;
    /// it must load (the value is ignored), not be quarantined as invalid.
    #[test]
    fn a_legacy_usage_window_key_is_ignored_on_load() {
        let cfg: AppConfig =
            serde_json::from_str(r#"{"theme":"dark","usageWindow":"last7Days"}"#).unwrap();
        assert_eq!(cfg.theme, "dark");
    }

    /// "Open pricing file" must always open something useful, so a missing
    /// pricing.json is created from a template with an empty override table
    /// instead of the OS reporting a file-not-found error.
    #[test]
    fn ensure_pricing_file_creates_the_template_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pricing.json");
        ensure_pricing_file(&path).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"models\""), "template must be parseable pricing.json shape");
        // The template itself must be valid pricing.json, not just contain the word.
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["models"].is_object());
    }

    /// The user's own edits are theirs: an existing pricing.json, even one
    /// pricing.rs would consider malformed, must never be overwritten by
    /// "Open pricing file".
    #[test]
    fn ensure_pricing_file_never_overwrites_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pricing.json");
        fs::write(&path, r#"{"models":{"x":{"input":1.0,"output":1.0}}}"#).unwrap();
        ensure_pricing_file(&path).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"x\""), "existing user content must survive");
    }

    /// The config dir may not exist yet on a first run — `ensure_pricing_file`
    /// has to create it, the same as `config_dir` does for config.json.
    #[test]
    fn ensure_pricing_file_creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("pricing.json");
        ensure_pricing_file(&path).unwrap();
        assert!(path.exists());
    }

    /// The shipped bar demonstrates launch actions: one cell opens the project
    /// page, and it sits right before the settings cell.
    #[test]
    fn default_items_include_the_github_cell_before_settings() {
        let items = default_items();
        let pos = items.iter().position(|i| i.action == "open:https://github.com/montesgp/orbitbar");
        let pos = pos.expect("default items must carry the GitHub cell");
        assert_eq!(items[pos].id, "github");
        assert_eq!(items[pos + 1].id, "settings");
    }

    /// The default set must stay usable: the bar renders nothing if there is
    /// not at least one item.
    #[test]
    fn default_items_are_never_empty_and_carry_glyphs() {
        let items = default_items();
        assert!(!items.is_empty());
        for item in &items {
            assert!(!item.id.is_empty());
            // Glyphs are stored as "0x2733"; the frontend parses them with
            // parseInt(.., 16), which tolerates the 0x prefix, so the check
            // here strips it too rather than using from_str_radix directly.
            let hex = item.glyph.trim_start_matches("0x");
            assert!(
                u32::from_str_radix(hex, 16).is_ok(),
                "glyph {} must be a hex code point",
                item.glyph
            );
            assert!(!item.tooltip.is_empty(), "{} needs a tooltip", item.id);
        }
    }

    /// Examples only demonstrate what cells can do; nobody sees them until
    /// they opt in, and an old config.json without the key stays opted out.
    #[test]
    fn examples_are_hidden_by_default() {
        assert!(!AppConfig::default().show_examples);
        let cfg: AppConfig = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert!(!cfg.show_examples, "an old config must not turn examples on");
    }

    #[test]
    fn an_explicit_show_examples_true_is_preserved() {
        let cfg: AppConfig = serde_json::from_str(r#"{"showExamples":true}"#).unwrap();
        assert!(cfg.show_examples);
    }

    /// Old configs have no saved position; that means "place it right-center".
    #[test]
    fn a_config_without_position_loads_with_none_and_writes_none_back() {
        let cfg: AppConfig = serde_json::from_str(r#"{"theme":"dark","monitor":"primary","margin":8}"#).unwrap();
        assert_eq!(cfg.position, None);
        assert_eq!(cfg.margin, 8, "monitor/margin keep working next to the new field");
        assert!(!serde_json::to_string(&cfg).unwrap().contains("position"));
        let null: AppConfig = serde_json::from_str(r#"{"position":null}"#).unwrap();
        assert_eq!(null.position, None);
    }

    /// The dropped position survives a save and a reload.
    #[test]
    fn a_saved_position_round_trips_including_negative_coordinates() {
        let cfg: AppConfig = serde_json::from_str(r#"{"position":{"x":-1500,"y":120}}"#).unwrap();
        assert_eq!(cfg.position, Some(Position { x: -1500, y: 120 }));
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains(r#""position":{"x":-1500,"y":120}"#), "{json}");
        let again: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(again.position, cfg.position);
    }

    /// The GitHub cell is the shipped example, and it says so.
    #[test]
    fn the_default_github_cell_is_flagged_as_an_example() {
        let items = default_items();
        let github = items.iter().find(|i| i.id == "github").expect("github cell");
        assert!(github.example);
        assert!(github.tooltip.starts_with("Example action"), "{}", github.tooltip);
        assert!(items.iter().filter(|i| i.example).count() == 1, "only github is an example");
    }

    /// An item written before `example` existed must still load, as a normal
    /// cell, and a normal cell is written back without the key.
    #[test]
    fn items_without_example_load_and_serialize_without_it() {
        let item: Item = serde_json::from_str(
            r#"{"id":"a","label":"a","glyph":"0x41","action":"none","tooltip":"t"}"#,
        )
        .unwrap();
        assert!(!item.example);
        assert!(!serde_json::to_string(&item).unwrap().contains("example"));
        let flagged: Item = serde_json::from_str(
            r#"{"id":"a","label":"a","glyph":"0x41","action":"none","tooltip":"t","example":true}"#,
        )
        .unwrap();
        assert!(flagged.example);
        assert!(serde_json::to_string(&flagged).unwrap().contains("\"example\":true"));
    }
}
