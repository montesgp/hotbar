//! Orbitbar config v2 — schema, loader, and theme palettes as data.
//!
//! The config lives in the per-user app config dir (Tauri `app_config_dir`):
//!   Windows: %APPDATA%\com.orbitbar.app\config.json
//!   Linux:   ~/.config/com.orbitbar.app/config.json
//!   macOS:   ~/Library/Application Support/com.orbitbar.app/config.json
//!
//! If the file does not exist on first launch it is created with the defaults
//! below, so end users always get an editable config without installing tools.
//!
//! The Tauri identifier used to be `com.hotbar.app`, which put the config in a
//! sibling directory under the same platform config root. `migrate_legacy_config`
//! copies that old file into the new location on first launch so a rename of
//! the app never drops an existing user's settings.

use crate::usage::TimeWindow;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
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
    /// The time window the usage panel (`agent-usage` / `agent-usage:<agent>`)
    /// reads by default and persists after the user changes the selector.
    /// Defaults to `ThisMonth`, matching the legacy widget's month-to-date
    /// panel; a config written before this field existed simply gets that
    /// default rather than failing to parse, the same migration-safe pattern
    /// `auto_start` uses above.
    pub usage_window: TimeWindow,
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
            auto_start: true,
            usage_window: TimeWindow::default(),
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

impl Default for ThemePalette {
    fn default() -> Self {
        Self {
            name: "classic".into(),
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

const PALETTE_CLASSIC: &str = r##"{
  "name": "classic",
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

const PALETTE_DARK: &str = r##"{
  "name": "dark",
  "background": "#0D0D11",
  "panel": "#1F1F26",
  "text": "#E6E6F0",
  "textDim": "#9A9AA8",
  "hoverBg": "#2A2A35",
  "hoverFg": "#FFFFFF",
  "radiusBar": 12.0,
  "radiusCell": 12.0,
  "radiusHandle": 8.0,
  "tabRadius": 12.0,
  "barBorder": "#1F1F26",
  "gradientTop": "#0D0D11",
  "gradientBottom": "#0D0D11",
  "halfMoon": false,
  "moonRadius": 12.0
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

    if let Some(legacy_dir) = legacy_config_dir(app) {
        let legacy_path = legacy_dir.join("config.json");
        // Best-effort: a migration failure (e.g. unreadable old file) must not
        // block startup. Falling through to the ordinary default-writer below
        // is strictly better than refusing to launch.
        let _ = migrate_legacy_config(&legacy_path, &path);
    }

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

fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let resolver = app.path();
    let dir = resolver
        .app_config_dir()
        .map_err(|e| format!("cannot resolve app config dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// The pre-rename per-user config dir, if it can be resolved. The Tauri
/// identifier used to be `com.hotbar.app`, a sibling of the current
/// `com.orbitbar.app` under the same platform config root, so this never
/// calls `app_config_dir()` itself (that resolves the *current* identifier)
/// and instead derives the sibling path from it.
fn legacy_config_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    let parent = dir.parent()?;
    Some(parent.join("com.hotbar.app"))
}

/// Copies `old_path` into `new_path` when the new config does not exist yet
/// but the old one does. Returns whether a migration happened. The old file
/// is left in place (copy, not move) so a rollback to a previous build still
/// finds its config.
fn migrate_legacy_config(old_path: &Path, new_path: &Path) -> Result<bool, String> {
    if new_path.exists() || !old_path.exists() {
        return Ok(false);
    }
    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    fs::copy(old_path, new_path)
        .map_err(|e| format!("cannot copy {} to {}: {e}", old_path.display(), new_path.display()))?;
    Ok(true)
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
            tooltip: format!("Claude - month history and per-project ({tooltip})"),
        },
        Item {
            id: "codex".into(),
            label: "codex".into(),
            glyph: "0x25CE".into(),
            action: "agent-usage:codex".into(),
            tooltip: format!("Codex - month history and per-project ({tooltip})"),
        },
        Item {
            id: "opencode".into(),
            label: "opencode".into(),
            glyph: "0x25C8".into(),
            action: "agent-usage:opencode".into(),
            tooltip: format!("Opencode - month history and per-project ({tooltip})"),
        },
        Item {
            id: "usage".into(),
            label: "usage".into(),
            glyph: "0x0024".into(),
            action: "agent-usage".into(),
            tooltip: "Live session usage (claude/codex/opencode) - real-time balance".into(),
        },
        Item {
            id: "settings".into(),
            label: "settings".into(),
            glyph: "0x2699".into(),
            action: "edit-config".into(),
            tooltip: "Open orbitbar config".into(),
        },
        Item {
            id: "autostart".into(),
            label: "autostart".into(),
            // U+23FB POWER SYMBOL. The state is carried by dimming and the
            // tooltip, not by a second glyph, so the icon stays the user's.
            glyph: "0x23FB".into(),
            action: "toggle-autostart".into(),
            tooltip: "Launch Orbitbar at login".into(),
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
        for theme in ["classic", "dark"] {
            let p = palette_for(theme);
            assert_eq!(p.name, theme);
            assert!(!p.background.is_empty());
            assert!(p.radius_cell > 0.0);
        }
    }

    /// A typo in the config theme must never break the bar.
    #[test]
    fn unknown_theme_falls_back_to_classic() {
        assert_eq!(palette_for("clasci").name, "classic");
        assert_eq!(palette_for("").name, "classic");
    }

    /// The crescent only works because the moon radius matches the bar height;
    /// a regression here silently turns the bar into a rounded rectangle.
    #[test]
    fn classic_moon_geometry_matches_the_bar() {
        let p = palette_for("classic");
        assert!(p.half_moon);
        assert_eq!(p.moon_radius, 200.0, "moon radius is half the 400px bar height");
        assert_eq!(p.tab_radius, 23.0, "collapsed tab is 46x46, so half is 23");
    }

    /// Cells are 44px circles in the WPF original, not 8px rounded squares.
    #[test]
    fn cell_radius_is_a_circle_not_a_rounded_square() {
        assert_eq!(palette_for("classic").radius_cell, 22.0);
    }

    /// A BOM-edited config is a normal thing on Windows: Notepad and
    /// PowerShell 5.1 both add one, and serde_json rejects it at byte 0.
    #[test]
    fn strip_bom_lets_a_bom_edited_config_parse() {
        let clean = r#"{"monitor":"primary","margin":8,"collapsed":false,"theme":"classic","fontSize":10.0,"items":[]}"#;
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
        let cfg: AppConfig = serde_json::from_str(r#"{"theme":"classic"}"#).unwrap();
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

    /// A config written before `usageWindow` existed must migrate to
    /// `ThisMonth` rather than fail to parse, so the usage panel opens with a
    /// sane default on an upgrade instead of bricking the config load.
    #[test]
    fn a_config_without_usage_window_defaults_to_this_month() {
        let cfg: AppConfig = serde_json::from_str(r#"{"theme":"classic"}"#).unwrap();
        assert_eq!(cfg.usage_window, TimeWindow::ThisMonth);
    }

    /// An explicit selector must be preserved across save/load, otherwise the
    /// window choice the user made in the panel would never stick.
    #[test]
    fn an_explicit_usage_window_is_preserved() {
        let cfg: AppConfig = serde_json::from_str(r#"{"usageWindow":"last7Days"}"#).unwrap();
        assert_eq!(cfg.usage_window, TimeWindow::Last7Days);
    }

    /// The identifier rename (`com.hotbar.app` -> `com.orbitbar.app`) moves the
    /// per-user config dir. A user with an existing config under the old
    /// identifier must not lose it: the old file gets copied into the new
    /// location on first launch, before it is loaded.
    #[test]
    fn migrate_legacy_config_copies_old_into_new_when_only_old_exists() {
        let tmp = std::env::temp_dir().join(format!(
            "orbitbar-migrate-test-{}-a",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("old").join("config.json");
        let new_path = tmp.join("new").join("config.json");
        fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        fs::write(&old_path, r#"{"theme":"dark"}"#).unwrap();

        let migrated = migrate_legacy_config(&old_path, &new_path).unwrap();

        assert!(migrated, "must report that it migrated");
        assert!(new_path.exists(), "new config must now exist");
        assert!(old_path.exists(), "old config must be preserved, not moved");
        assert_eq!(
            fs::read_to_string(&new_path).unwrap(),
            fs::read_to_string(&old_path).unwrap()
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// When the new config already exists, the migration must not clobber it
    /// with the old one, even if the old one is still present.
    #[test]
    fn migrate_legacy_config_does_nothing_when_new_already_exists() {
        let tmp = std::env::temp_dir().join(format!(
            "orbitbar-migrate-test-{}-b",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("old").join("config.json");
        let new_path = tmp.join("new").join("config.json");
        fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        fs::create_dir_all(new_path.parent().unwrap()).unwrap();
        fs::write(&old_path, r#"{"theme":"dark"}"#).unwrap();
        fs::write(&new_path, r#"{"theme":"classic"}"#).unwrap();

        let migrated = migrate_legacy_config(&old_path, &new_path).unwrap();

        assert!(!migrated, "must not report a migration");
        assert_eq!(fs::read_to_string(&new_path).unwrap(), r#"{"theme":"classic"}"#);

        let _ = fs::remove_dir_all(&tmp);
    }

    /// When neither file exists, migration is a safe no-op: the ordinary
    /// first-launch default writer takes over from there.
    #[test]
    fn migrate_legacy_config_does_nothing_when_neither_exists() {
        let tmp = std::env::temp_dir().join(format!(
            "orbitbar-migrate-test-{}-c",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("old").join("config.json");
        let new_path = tmp.join("new").join("config.json");

        let migrated = migrate_legacy_config(&old_path, &new_path).unwrap();

        assert!(!migrated);
        assert!(!new_path.exists());

        let _ = fs::remove_dir_all(&tmp);
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
}