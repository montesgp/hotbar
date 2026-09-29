// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod config;
mod launch;
mod placement;
mod usage;

use serde::{Deserialize, Serialize};
use tauri::{Manager, WebviewWindow};
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

/// Token spend per agent (claude, codex, opencode) and per project over
/// `window`, read only from each agent's own local files - no external
/// service or program involved. Runs off the main thread: the readers stream
/// multi-megabyte jsonl files and query a SQLite database, which would
/// otherwise stall the window.
///
/// `pricing.json` next to `config.json` is (re)loaded on every call, not
/// cached, so editing it takes effect on the next refresh without an app
/// restart - the file is small and this runs off the main thread anyway.
#[tauri::command]
async fn get_usage(app: tauri::AppHandle, window: usage::TimeWindow) -> Result<usage::UsageSnapshot, String> {
    let pricing_path = config::config_dir(&app)?.join("pricing.json");
    tauri::async_runtime::spawn_blocking(move || {
        let paths = usage::resolve_paths(None).ok_or_else(|| "cannot resolve home directory".to_string())?;
        let (overrides, pricing_warning) = usage::pricing::load_overrides(&pricing_path);
        let mut snapshot = usage::collect_usage(window, &paths, chrono::Local::now(), &overrides);
        snapshot.pricing_warning = pricing_warning;
        Ok(snapshot)
    })
    .await
    .map_err(|e| format!("usage task panicked: {e}"))?
}

/// Full path to config.json, for the context menu's
/// "Edit config" to hand to the opener plugin. Returns the path even if the
/// file somehow does not exist yet — `get_config` always creates it first on
/// a real launch, so in practice this only runs after that.
#[tauri::command]
fn get_config_path(app: tauri::AppHandle) -> Result<String, String> {
    let dir = config::config_dir(&app)?;
    Ok(dir.join("config.json").to_string_lossy().into_owned())
}

/// Creates pricing.json from the built-in template if it is missing, then
/// returns its path, for "Open pricing file" to hand to the opener plugin.
/// Never overwrites an existing file - see `config::ensure_pricing_file`.
#[tauri::command]
fn ensure_pricing_file(app: tauri::AppHandle) -> Result<String, String> {
    let dir = config::config_dir(&app)?;
    let path = dir.join("pricing.json");
    config::ensure_pricing_file(&path)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Runs a cell's `run:<program> [args]` action: spawns the program directly
/// (no shell) and returns at once. The frontend sends only the item id; the
/// command line comes from the config loaded here, and an id that is unknown or
/// whose action is not `run:` is an error. Only an explicit cell click calls
/// this; nothing runs at startup. See `launch::command_for_item`.
#[tauri::command]
fn run_command(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let cfg = config::load(&app)?;
    launch::spawn(&launch::command_for_item(&cfg.items, &id)?)
}

/// Exits the whole process, called by the context menu's "Quit Orbitbar".
/// `app.exit(0)` tears the app down through Tauri's own shutdown path
/// (closes every window, runs `on_exit` if one is ever added); a bare
/// `window.close()` on the frontend only removes the window and leaves the
/// process running in the background, which is the bug this command fixes.
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
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

/// What `place_window` is asked to lay out, all in physical pixels.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceTarget {
    /// The bar as it is now (screen rectangle, without any menu or panel).
    bar: placement::Rect,
    /// The size the bar should have: the current one, or the other state's
    /// when collapsing or expanding.
    bar_width: u32,
    bar_height: u32,
    /// Room the context menu or usage panel needs beside the bar. The height
    /// is a minimum: the window is never shorter than the bar.
    extra_width: u32,
    extra_height: u32,
}

/// The placement plus the name of the monitor the bar is on, so the frontend
/// can persist both after a drag.
#[derive(Serialize)]
struct PlaceResult {
    #[serde(flatten)]
    placement: placement::Placement,
    monitor: Option<String>,
}

/// Work area of a monitor. The work area is the monitor minus the taskbar and
/// other docked toolbars, so nothing is placed under them.
fn work_rect(monitor: &tauri::Monitor) -> placement::Rect {
    let area = monitor.work_area();
    placement::Rect::new(area.position.x, area.position.y, area.size.width, area.size.height)
}

/// Lays the window out for the bar plus whatever is open beside it, applying
/// position AND size in one native operation, and returns where everything
/// went (see `placement::place`).
///
/// The bar never moves on screen: the window grows toward the side of the bar
/// that has room and the frontend draws the menu/panel there. A bar that is
/// outside its monitor is clamped in, so the same call also settles a drag.
///
/// Doing it as separate `set_size` / `set_position` calls lets the compositor
/// show the in-between frame (new size at the old position), which is a visible
/// jump. On Windows this is one `SetWindowPos`; elsewhere it is `set_size` then
/// `set_position`, the best the platform API offers. See `set_bounds` for how
/// each platform deals with the window being `resizable: false`.
#[tauri::command]
fn place_window(window: WebviewWindow, target: PlaceTarget) -> Result<PlaceResult, String> {
    let result = compute_placement(&window, &target)?;
    let w = result.placement.window;
    set_bounds(&window, w.x, w.y, w.width, w.height).map_err(|e| e.to_string())?;
    Ok(result)
}

/// The same answer as `place_window` without touching the window. The
/// frontend asks first so it can put the menu/panel on the right side of the
/// bar BEFORE the window grows: growing to the right of a bar the page still
/// draws at the far right would show it jumping for a frame.
#[tauri::command]
fn plan_window(window: WebviewWindow, target: PlaceTarget) -> Result<PlaceResult, String> {
    compute_placement(&window, &target)
}

/// Picks the monitor the bar is on and runs the pure placement on its work
/// area.
fn compute_placement(window: &WebviewWindow, target: &PlaceTarget) -> Result<PlaceResult, String> {
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    if monitors.is_empty() {
        return Err("no monitor is available".to_string());
    }
    let areas: Vec<placement::Rect> = monitors.iter().map(work_rect).collect();
    // A bar that overlaps no monitor (one was unplugged) goes to the primary.
    let primary = window
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .and_then(|p| monitors.iter().position(|m| m.position() == p.position()))
        .unwrap_or(0);
    let index = placement::pick_area(target.bar, &areas).unwrap_or(primary);

    let placed = placement::place(
        target.bar,
        (target.bar_width, target.bar_height),
        (target.extra_width, target.extra_height),
        areas[index],
    );
    Ok(PlaceResult { placement: placed, monitor: monitors[index].name().cloned() })
}

/// Applies position and size to the window in one native call on Windows.
///
/// The window is `resizable: false`. On Windows that only means the frame has
/// no `WS_SIZEBOX` (no resize borders); tao does not clamp programmatic sizes
/// (`WM_WINDOWPOSCHANGING` passes them through and the min/max constraints
/// only affect user-driven resizing), so `SetWindowPos` may resize the window
/// as it is. Toggling `resizable` around the call used to be done here, but in
/// tao that rewrites the window style (`SetWindowLongW`), forces
/// `SWP_FRAMECHANGED` (a full non-client recalculation and repaint) and
/// attaches/detaches tauri's undecorated-resize hook, twice per placement:
/// the frame flash seen when picking a menu entry.
#[cfg(windows)]
fn set_bounds(window: &WebviewWindow, x: i32, y: i32, width: u32, height: u32) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

    let hwnd = window.hwnd()?;
    // SAFETY: `hwnd` is the live handle of this window; the call has no
    // pointer arguments beyond the handle itself. `SWP_NOCOPYBITS` is left
    // off on purpose: the webview is a child surface composed by DWM, so
    // nothing stale is blitted, and discarding the client bits could itself
    // produce a blank frame.
    let ok = unsafe {
        SetWindowPos(hwnd.0 as _, std::ptr::null_mut(), x, y, width as i32, height as i32, SWP_NOZORDER | SWP_NOACTIVATE)
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_bounds(window: &WebviewWindow, x: i32, y: i32, width: u32, height: u32) -> tauri::Result<()> {
    use tauri::{PhysicalPosition, PhysicalSize};

    // Unlike Windows, the toolkit here may pin min/max size to the current size
    // while the window is `resizable: false` (GTK size hints), so release the
    // lock around the change and take it again at the new size. There is no
    // single native call here, so size and position stay two steps. Not
    // verified on these platforms; the Windows path above does not need it.
    window.set_resizable(true)?;
    let moved = window
        .set_size(PhysicalSize::new(width, height))
        .and_then(|()| window.set_position(PhysicalPosition::new(x, y)));
    let relock = window.set_resizable(false);
    moved.and(relock)
}

#[derive(Debug, PartialEq, Eq)]
enum AutostartAction {
    Enable,
    Disable,
    Nothing,
}

/// Decides what to do with the OS autostart entry. The config is the source of
/// truth, reconciled on every launch:
///   - a debug build never touches the entry: it runs from target/debug and
///     needs the dev server, so registering it would open a blank window at
///     the next login;
///   - wanted -> always enable, even when an entry exists, because the entry
///     stores an executable path and rewriting it heals one left behind by an
///     older or moved build (the entry name alone cannot tell them apart);
///   - unwanted -> disable only when an entry is actually there.
///
/// `registered` is `None` when the entry could not be read.
fn autostart_action(debug_build: bool, wanted: bool, registered: Option<bool>) -> AutostartAction {
    if debug_build {
        return AutostartAction::Nothing;
    }
    match (wanted, registered) {
        (true, _) => AutostartAction::Enable,
        (false, Some(true)) => AutostartAction::Disable,
        (false, _) => AutostartAction::Nothing,
    }
}

/// Failing to register is never fatal. An orbitbar that refuses to launch
/// because a Run key could not be written is strictly worse than one that
/// launches without autostart, so this logs and lets startup continue.
fn sync_autostart(app: &tauri::AppHandle, cfg: &config::AppConfig) {
    let manager = app.autolaunch();
    let registered = manager.is_enabled().ok();
    let outcome = match autostart_action(cfg!(debug_assertions), cfg.auto_start, registered) {
        AutostartAction::Enable => manager.enable(),
        AutostartAction::Disable => manager.disable(),
        AutostartAction::Nothing => Ok(()),
    };
    if let Err(e) = outcome {
        eprintln!("autostart could not be reconciled with config: {e}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // Registered first, as the plugin requires: a second launch exits at once
    // and this callback runs in the instance that is already running, which
    // brings its bar forward instead of leaving two bars on screen.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }));

    builder
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

            // Start in the persisted state: collapsed is a small chevron tab,
            // expanded is the full launcher. The bar goes where it was last
            // dropped; with no saved position, or one that is off every
            // monitor, it goes right-center of the configured monitor with the
            // configured margin, as on a first launch.
            //
            // The size is forced together with the position, never left to the
            // window as created: Windows was handing back a 136x400 window
            // (the webview's min-content width plus a frame), which pushed 56px
            // off the right edge of a 1080 monitor and made the right-center
            // placement aim at an edge that was not there.
            let size = if cfg.collapsed { SIZE_COLLAPSED } else { SIZE_EXPANDED };
            let areas: Vec<placement::Rect> = window.available_monitors()?.iter().map(work_rect).collect();
            let fallback = work_rect(&resolve_monitor(&window, &cfg.monitor)?);
            let bar = placement::restore_bar(cfg.position.map(|p| (p.x, p.y)), size, &areas, fallback, cfg.margin);
            set_bounds(&window, bar.x, bar.y, bar.width, bar.height)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_usage,
            quit_app,
            get_config_path,
            ensure_pricing_file,
            run_command,
            place_window,
            plan_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod autostart_tests {
    use super::{autostart_action, AutostartAction};

    /// A dev build runs from target/debug and needs the Vite dev server. If it
    /// registered itself, the next login would open a blank window.
    #[test]
    fn a_debug_build_never_touches_the_os_entry() {
        assert_eq!(autostart_action(true, true, Some(false)), AutostartAction::Nothing);
        assert_eq!(autostart_action(true, false, Some(true)), AutostartAction::Nothing);
    }

    /// The OS entry stores an executable path. Re-enabling on every launch
    /// rewrites it to the binary that is actually running, so an entry left
    /// behind by an older or moved build heals itself.
    #[test]
    fn a_wanted_entry_is_rewritten_even_when_one_exists() {
        assert_eq!(autostart_action(false, true, Some(true)), AutostartAction::Enable);
        assert_eq!(autostart_action(false, true, Some(false)), AutostartAction::Enable);
    }

    /// Not being able to read the entry is no reason to skip registering it.
    #[test]
    fn an_unreadable_entry_is_still_enabled_when_wanted() {
        assert_eq!(autostart_action(false, true, None), AutostartAction::Enable);
    }

    #[test]
    fn an_unwanted_entry_is_removed_only_when_present() {
        assert_eq!(autostart_action(false, false, Some(true)), AutostartAction::Disable);
        assert_eq!(autostart_action(false, false, Some(false)), AutostartAction::Nothing);
        assert_eq!(autostart_action(false, false, None), AutostartAction::Nothing);
    }
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