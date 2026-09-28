// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod config;

use serde::Serialize;
use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_autostart::ManagerExt;

/// Physical window sizes for the two visual states (must match src/main.ts).
/// The expanded bar is a 72x400 crescent and the collapsed state is a 46x46
/// tab, both ported from the measured WPF v1 window. tauri.conf.json declares
/// the same expanded size so the window is born at the right shape.
const SIZE_COLLAPSED: (u32, u32) = (46, 46);
const SIZE_EXPANDED: (u32, u32) = (72, 400);

#[derive(Serialize)]
struct ConfigPayload {
    config: config::AppConfig,
    palette: config::ThemePalette,
}

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> Result<ConfigPayload, String> {
    let cfg = config::load(&app)?;
    let palette = config::palette_for(&cfg.theme);
    Ok(ConfigPayload { config: cfg, palette })
}

#[tauri::command]
fn save_config(app: tauri::AppHandle, cfg: config::AppConfig) -> Result<(), String> {
    config::save(&app, &cfg)
}

/// Resolve the monitor named in config ("primary" or a device name). Falls
/// back to the current monitor when the persisted name no longer exists.
fn resolve_monitor(
    window: &WebviewWindow,
    wanted: &str,
) -> tauri::Result<tauri::Monitor> {
    let monitors = window.available_monitors()?;
    if let Some(m) = window.primary_monitor()? {
        if wanted.eq_ignore_ascii_case("primary") {
            return Ok(m);
        }
        if m
            .name()
            .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
        {
            return Ok(m);
        }
    }
    if let Some(m) = monitors
        .iter()
        .find(|m| m.name().is_some_and(|name| name.eq_ignore_ascii_case(wanted)))
    {
        return Ok(m.clone());
    }
    Ok(window.current_monitor()?.unwrap_or(monitors[0].clone()))
}

/// Position a window right-center of a monitor with the config margin.
fn position_right_center(
    window: &WebviewWindow,
    monitor: &tauri::Monitor,
    margin: i32,
    size: PhysicalSize<u32>,
) -> tauri::Result<()> {
    let m_pos = monitor.position();
    let m_size = monitor.size();
    let x = m_pos.x + (m_size.width as i32 - size.width as i32) - margin;
    let y = m_pos.y + (m_size.height as i32 - size.height as i32) / 2;
    window.set_position(PhysicalPosition::new(x, y))
}

/// The config is the source of truth for autostart, so reconcile the OS entry
/// against it on every launch instead of only ever writing it once. Three cases
/// matter and only two need a write:
///   - fresh install, autoStart defaults to true, no entry exists  -> enable
///   - the user turned autostart off in their OS settings          -> enable again
///   - the user turned it off here, entry still registered        -> disable
///
/// Failing to register is never fatal. An orbitbar that refuses to launch
/// because a Run key could not be written is strictly worse than one that
/// launches without autostart, so this logs and lets startup continue.
fn sync_autostart(app: &tauri::AppHandle, cfg: &config::AppConfig) {
    let manager = app.autolaunch();
    let outcome = match manager.is_enabled() {
        Ok(true) if !cfg.auto_start => manager.disable(),
        Ok(false) if cfg.auto_start => manager.enable(),
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    };
    match outcome {
        Ok(()) => {}
        Err(e) => eprintln!("autostart could not be reconciled with config: {e}"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("Orbitbar")
                .build(),
        )
        .setup(|app| {
            let window = app.get_webview_window("main").expect("main window");
            let cfg = config::load(app.handle())?;
            sync_autostart(app.handle(), &cfg);

            // Start in the persisted state: collapsed is a small chevron bar,
            // expanded is the full launcher; both sit right-center on the
            // persisted monitor with the persisted margin.
            let size = if cfg.collapsed {
                PhysicalSize::new(SIZE_COLLAPSED.0, SIZE_COLLAPSED.1)
            } else {
                PhysicalSize::new(SIZE_EXPANDED.0, SIZE_EXPANDED.1)
            };
            // Always force the size, not just when collapsed. Windows was
            // handing back a 136x400 window (the webview's min-content width
            // plus a frame), which pushed 56px off the right edge of a 1080
            // monitor and made position_right_center aim at an edge that was
            // not there. Setting it unconditionally keeps the two in sync.
            window.set_size(size)?;

            let monitor = resolve_monitor(&window, &cfg.monitor)?;
            position_right_center(&window, &monitor, cfg.margin, size)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_config, save_config])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{SIZE_COLLAPSED, SIZE_EXPANDED};

    /// The crescent radii src/main.ts writes into `--ob-moon-rx` / `--ob-moon-ry`.
    fn moon_radii() -> (u32, u32) {
        (SIZE_EXPANDED.0, SIZE_EXPANDED.1 / 2)
    }

    #[test]
    fn expanded_bar_keeps_the_measured_wpf_geometry() {
        assert_eq!(SIZE_EXPANDED, (72, 400));
        assert_eq!(SIZE_COLLAPSED, (46, 46));
    }

    /// True when CSS renders `rx ry` untouched. CSS rescales every radius on an
    /// element when any of them overflows their edge, so the box is 72 wide
    /// against 200 of horizontal radius and the crescent silently becomes a
    /// circle. Two edges constrain a left-only corner: the top edge carries the
    /// single left radius, the left edge carries both vertical radii.
    fn css_keeps_radii(rx: u32, ry: u32, width: u32, height: u32) -> bool {
        rx <= width && ry * 2 <= height
    }

    #[test]
    fn moon_radii_survive_css_unscaled() {
        let (rx, ry) = moon_radii();
        assert!(
            css_keeps_radii(rx, ry, SIZE_EXPANDED.0, SIZE_EXPANDED.1),
            "rx {rx} ry {ry} would be rescaled on a {}x{} bar",
            SIZE_EXPANDED.0,
            SIZE_EXPANDED.1
        );
    }

    /// Proves the check above has teeth. The WPF `CornerRadius="200,0,0,200"`
    /// looks equivalent but is not: 200px of horizontal radius on a 72px bar
    /// triggers f = 0.36 and renders a circle. If this ever stops holding, the
    /// guard is not testing what it claims to.
    #[test]
    fn the_wpf_style_200px_shorthand_would_be_rescaled() {
        assert!(!css_keeps_radii(200, 200, SIZE_EXPANDED.0, SIZE_EXPANDED.1));
    }

    /// The collapsed tab is a half circle on the left of a 46x46 window, same rule.
    #[test]
    fn collapsed_tab_radii_never_overflow() {
        let r = SIZE_COLLAPSED.0 / 2;
        assert!(css_keeps_radii(r, r, SIZE_COLLAPSED.0, SIZE_COLLAPSED.1));
    }
}