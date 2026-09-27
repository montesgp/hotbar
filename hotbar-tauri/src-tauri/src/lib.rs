// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod config;

use serde::Serialize;
use tauri::{Manager, PhysicalPosition};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Position the hotbar right-center of the current (active) monitor,
            // matching the WPF widget behavior.
            let window = app.get_webview_window("main").expect("main window");
            if let Some(monitor) = window.current_monitor()? {
                let m_pos = monitor.position(); // PhysicalPosition<i32>
                let m_size = monitor.size(); // PhysicalSize<u32>
                let w_size = window.outer_size()?; // PhysicalSize<u32>
                let margin = 24;
                let x = m_pos.x + (m_size.width as i32 - w_size.width as i32) - margin;
                let y = m_pos.y + (m_size.height as i32 - w_size.height as i32) / 2;
                window.set_position(PhysicalPosition::new(x, y))?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_config, save_config])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}