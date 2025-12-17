use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

use crate::config;
use crate::types::CaptureMode;

/// Parse shortcut string to Shortcut struct (e.g., "Alt+A" -> Shortcut)
pub fn parse_shortcut(s: &str) -> Result<Shortcut, String> {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() {
        return Err("Empty shortcut".to_string());
    }

    let key_str = parts.last().ok_or("No key")?;
    let key_code = match key_str.to_uppercase().as_str() {
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "0" => Code::Digit0,
        "ESCAPE" | "ESC" => Code::Escape,
        _ => return Err(format!("Unknown key: {}", key_str)),
    };

    let mut modifiers = Modifiers::empty();
    for part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "alt" | "option" | "opt" => modifiers |= Modifiers::ALT,
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "super" | "meta" | "cmd" | "command" => modifiers |= Modifiers::SUPER,
            _ => return Err(format!("Unknown modifier: {}", part)),
        }
    }

    let mods = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };
    Ok(Shortcut::new(mods, key_code))
}

/// Get action from shortcut (reverse lookup)
pub fn get_action_for_shortcut(shortcut: &Shortcut) -> Option<CaptureMode> {
    let config = config::load_config();

    for (action, cfg) in &config.shortcuts {
        if !cfg.enabled {
            continue;
        }
        let shortcut_str = cfg.to_shortcut_string();
        if let Ok(parsed) = parse_shortcut(&shortcut_str) {
            if &parsed == shortcut {
                return match action.as_str() {
                    "screenshot" => Some(CaptureMode::Image),
                    "gif" => Some(CaptureMode::Gif),
                    "video" => Some(CaptureMode::Video),
                    "scroll" => Some(CaptureMode::Scroll),
                    _ => None,
                };
            }
        }
    }
    None
}

/// Format shortcut for display (e.g., "Alt+A" -> "⌥A")
pub fn format_shortcut_display(s: &str) -> String {
    s.replace("Alt+", "⌥")
        .replace("Ctrl+", "⌃")
        .replace("Shift+", "⇧")
        .replace("Cmd+", "⌘")
        .replace("Command+", "⌘")
        .replace("Super+", "⌘")
        .replace("Meta+", "⌘")
}

/// Register shortcuts from config (called at startup and when config changes)
pub fn register_shortcuts_from_config(app: &AppHandle) -> Result<(), String> {
    let config = config::load_config();

    if let Err(e) = app.global_shortcut().unregister_all() {
        eprintln!("[shortcuts] Failed to unregister all: {}", e);
    }

    for (action, shortcut_cfg) in &config.shortcuts {
        if !shortcut_cfg.enabled {
            continue;
        }

        let shortcut_str = shortcut_cfg.to_shortcut_string();
        match parse_shortcut(&shortcut_str) {
            Ok(shortcut) => {
                if let Err(e) = app.global_shortcut().register(shortcut) {
                    eprintln!(
                        "[shortcuts] Failed to register {} ({}): {}",
                        action, shortcut_str, e
                    );
                } else {
                    println!("[shortcuts] Registered {} -> {}", action, shortcut_str);
                }
            }
            Err(e) => {
                eprintln!("[shortcuts] Invalid shortcut for {}: {}", action, e);
            }
        }
    }

    Ok(())
}
