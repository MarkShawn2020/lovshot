use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::config::{self, AppConfig, ShortcutConfig, WatermarkPosition};
use crate::shortcuts::register_shortcuts_from_config;
use crate::state::SharedState;
use crate::tray::update_tray_menu;

#[tauri::command]
pub fn get_shortcuts_config() -> AppConfig {
    config::load_config()
}

/// Save all shortcuts for an action (replaces existing)
#[tauri::command]
pub fn save_shortcut(
    app: AppHandle,
    action: String,
    shortcuts: Vec<ShortcutConfig>,
) -> Result<AppConfig, String> {
    let new_config = config::update_shortcuts(&action, shortcuts)?;
    register_shortcuts_from_config(&app)?;
    update_tray_menu(&app);

    Ok(new_config)
}

/// Add a single shortcut to an action
#[tauri::command]
pub fn add_shortcut(
    app: AppHandle,
    action: String,
    shortcut: ShortcutConfig,
) -> Result<AppConfig, String> {
    let new_config = config::add_shortcut(&action, shortcut)?;
    register_shortcuts_from_config(&app)?;
    update_tray_menu(&app);

    Ok(new_config)
}

/// Remove a shortcut from an action by index
#[tauri::command]
pub fn remove_shortcut(
    app: AppHandle,
    action: String,
    index: usize,
) -> Result<AppConfig, String> {
    let new_config = config::remove_shortcut(&action, index)?;
    register_shortcuts_from_config(&app)?;
    update_tray_menu(&app);

    Ok(new_config)
}

#[tauri::command]
pub fn reset_shortcuts_to_default(app: AppHandle) -> Result<AppConfig, String> {
    let config = AppConfig::default();
    config::save_config(&config)?;
    register_shortcuts_from_config(&app)?;
    update_tray_menu(&app);

    Ok(config)
}

#[tauri::command]
pub fn set_developer_mode(app: AppHandle, enabled: bool) -> Result<AppConfig, String> {
    let mut cfg = config::load_config();
    cfg.developer_mode = enabled;
    config::save_config(&cfg)?;
    update_tray_menu(&app);
    Ok(cfg)
}

#[tauri::command]
pub fn set_scroll_capture_enabled(app: AppHandle, enabled: bool) -> Result<AppConfig, String> {
    let mut cfg = config::load_config();
    cfg.scroll_capture_enabled = enabled;
    config::save_config(&cfg)?;
    register_shortcuts_from_config(&app)?;
    update_tray_menu(&app);
    Ok(cfg)
}

#[tauri::command]
pub fn set_screenshot_preview_enabled(enabled: bool) -> Result<AppConfig, String> {
    let mut cfg = config::load_config();
    cfg.screenshot_preview_enabled = enabled;
    config::save_config(&cfg)?;
    Ok(cfg)
}

#[tauri::command]
pub fn pause_shortcuts(app: AppHandle, state: tauri::State<SharedState>) -> Result<(), String> {
    {
        let mut s = state.lock().unwrap();
        s.shortcuts_paused_for_editing = true;
    }

    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    println!("[shortcuts] Paused all shortcuts for editing");
    Ok(())
}

#[tauri::command]
pub fn resume_shortcuts(app: AppHandle, state: tauri::State<SharedState>) -> Result<(), String> {
    let paused_for_tray_menu = {
        let mut s = state.lock().unwrap();
        s.shortcuts_paused_for_editing = false;
        s.shortcuts_paused_for_tray_menu
    };

    if paused_for_tray_menu {
        println!("[shortcuts] Resume requested but tray menu is open; deferring");
        return Ok(());
    }

    register_shortcuts_from_config(&app)?;
    println!("[shortcuts] Resumed shortcuts");
    Ok(())
}

#[tauri::command]
pub fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    let autostart = app.autolaunch();
    autostart.is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<AppConfig, String> {
    let autostart = app.autolaunch();

    if enabled {
        autostart.enable().map_err(|e| e.to_string())?;
    } else {
        autostart.disable().map_err(|e| e.to_string())?;
    }

    let mut cfg = config::load_config();
    cfg.autostart_enabled = enabled;
    config::save_config(&cfg)?;
    Ok(cfg)
}

#[tauri::command]
pub fn get_image_export_format() -> String {
    config::load_config().image_export_format
}

#[tauri::command]
pub fn set_image_export_format(format: String) -> Result<AppConfig, String> {
    // Validate format
    if !["markdown", "writing", "html", "url_only"].contains(&format.as_str()) {
        return Err("Invalid format. Must be 'markdown', 'writing', 'html', or 'url_only'".to_string());
    }
    let mut cfg = config::load_config();
    cfg.image_export_format = format;
    config::save_config(&cfg)?;
    Ok(cfg)
}

#[tauri::command]
pub fn get_screenshot_count() -> u64 {
    config::count_screenshots()
}

#[tauri::command]
pub fn get_watermark_position() -> String {
    let pos = config::load_config().watermark_position;
    match pos {
        WatermarkPosition::None => "none",
        WatermarkPosition::Brand => "brand",
        WatermarkPosition::TopLeft => "top_left",
        WatermarkPosition::TopRight => "top_right",
        WatermarkPosition::BottomLeft => "bottom_left",
        WatermarkPosition::BottomRight => "bottom_right",
    }.to_string()
}

#[tauri::command]
pub fn set_watermark_position(position: String) -> Result<AppConfig, String> {
    let pos = match position.as_str() {
        "none" => WatermarkPosition::None,
        "brand" => WatermarkPosition::Brand,
        "top_left" => WatermarkPosition::TopLeft,
        "top_right" => WatermarkPosition::TopRight,
        "bottom_left" => WatermarkPosition::BottomLeft,
        "bottom_right" => WatermarkPosition::BottomRight,
        _ => return Err("Invalid position".to_string()),
    };
    let mut cfg = config::load_config();
    cfg.watermark_position = pos;
    config::save_config(&cfg)?;
    Ok(cfg)
}
