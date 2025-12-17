use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::config::{self, AppConfig, ShortcutConfig};
use crate::shortcuts::register_shortcuts_from_config;
use crate::state::SharedState;

#[tauri::command]
pub fn get_shortcuts_config() -> AppConfig {
    config::load_config()
}

#[tauri::command]
pub fn save_shortcut(
    app: AppHandle,
    action: String,
    shortcut_str: String,
) -> Result<AppConfig, String> {
    let shortcut =
        ShortcutConfig::from_shortcut_string(&shortcut_str).ok_or("Invalid shortcut format")?;

    let new_config = config::update_shortcut(&action, shortcut)?;
    register_shortcuts_from_config(&app)?;

    Ok(new_config)
}

#[tauri::command]
pub fn reset_shortcuts_to_default(app: AppHandle) -> Result<AppConfig, String> {
    let config = AppConfig::default();
    config::save_config(&config)?;
    register_shortcuts_from_config(&app)?;

    Ok(config)
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
