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

    for (action, shortcuts) in &config.shortcuts {
        // Skip stop_recording - it's handled locally by overlay, not as a capture mode
        if action == "stop_recording" {
            continue;
        }

        for cfg in shortcuts {
            if !cfg.enabled {
                continue;
            }
            let shortcut_str = cfg.to_shortcut_string();
            if let Ok(parsed) = parse_shortcut(&shortcut_str) {
                if &parsed == shortcut {
                    return match action.as_str() {
                        "screenshot" => Some(CaptureMode::Image),
                        "screenshot_static" => Some(CaptureMode::StaticImage),
                        "gif" => Some(CaptureMode::Gif),
                        "video" => Some(CaptureMode::Video),
                        "scroll" => Some(CaptureMode::Scroll),
                        _ => None,
                    };
                }
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

/// Check if a shortcut is a stop_recording shortcut
pub fn is_stop_recording_shortcut(shortcut: &Shortcut) -> bool {
    is_shortcut_for_action(shortcut, "stop_recording")
}

/// Check if a shortcut is a show_main shortcut
pub fn is_show_main_shortcut(shortcut: &Shortcut) -> bool {
    is_shortcut_for_action(shortcut, "show_main")
}

/// Generic check if a shortcut matches an action
fn is_shortcut_for_action(shortcut: &Shortcut, action: &str) -> bool {
    let config = config::load_config();
    if let Some(shortcuts) = config.shortcuts.get(action) {
        for cfg in shortcuts {
            if !cfg.enabled {
                continue;
            }
            let shortcut_str = cfg.to_shortcut_string();
            if let Ok(parsed) = parse_shortcut(&shortcut_str) {
                if &parsed == shortcut {
                    return true;
                }
            }
        }
    }
    false
}

/// Register shortcuts from config (called at startup and when config changes)
/// NOTE: stop_recording and stop_scroll shortcuts are NOT registered here - they are
/// dynamically registered/unregistered when recording/scroll capture starts/stops
/// to avoid hijacking ESC globally
pub fn register_shortcuts_from_config(app: &AppHandle) -> Result<(), String> {
    let config = config::load_config();

    if let Err(e) = app.global_shortcut().unregister_all() {
        eprintln!("[shortcuts] Failed to unregister all: {}", e);
    }

    for (action, shortcuts) in &config.shortcuts {
        // Skip stop_recording and stop_scroll - they are dynamically registered
        // only during recording/scroll capture to avoid hijacking ESC globally
        if action == "stop_recording" || action == "stop_scroll" {
            continue;
        }

        // Skip scroll if not enabled (developer mode preview feature)
        if action == "scroll" && !config.scroll_capture_enabled {
            continue;
        }

        for shortcut_cfg in shortcuts {
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
    }

    Ok(())
}

/// Register stop_recording shortcuts (call when recording starts)
pub fn register_stop_shortcuts(app: &AppHandle) {
    let config = config::load_config();
    if let Some(shortcuts) = config.shortcuts.get("stop_recording") {
        for cfg in shortcuts {
            if !cfg.enabled {
                continue;
            }
            let shortcut_str = cfg.to_shortcut_string();
            if let Ok(shortcut) = parse_shortcut(&shortcut_str) {
                if let Err(e) = app.global_shortcut().register(shortcut) {
                    eprintln!("[shortcuts] Failed to register stop shortcut ({}): {}", shortcut_str, e);
                } else {
                    println!("[shortcuts] Registered stop_recording -> {}", shortcut_str);
                }
            }
        }
    }
}

/// Unregister stop_recording shortcuts (call when recording stops)
pub fn unregister_stop_shortcuts(app: &AppHandle) {
    let config = config::load_config();
    if let Some(shortcuts) = config.shortcuts.get("stop_recording") {
        for cfg in shortcuts {
            if !cfg.enabled {
                continue;
            }
            let shortcut_str = cfg.to_shortcut_string();
            if let Ok(shortcut) = parse_shortcut(&shortcut_str) {
                if let Err(e) = app.global_shortcut().unregister(shortcut) {
                    eprintln!("[shortcuts] Failed to unregister stop shortcut ({}): {}", shortcut_str, e);
                } else {
                    println!("[shortcuts] Unregistered stop_recording -> {}", shortcut_str);
                }
            }
        }
    }
}

/// Register stop_scroll shortcuts (call when scroll capture starts)
pub fn register_stop_scroll_shortcuts(app: &AppHandle) {
    let config = config::load_config();
    if let Some(shortcuts) = config.shortcuts.get("stop_scroll") {
        for cfg in shortcuts {
            if !cfg.enabled {
                continue;
            }
            let shortcut_str = cfg.to_shortcut_string();
            if let Ok(shortcut) = parse_shortcut(&shortcut_str) {
                if let Err(e) = app.global_shortcut().register(shortcut) {
                    eprintln!("[shortcuts] Failed to register stop_scroll shortcut ({}): {}", shortcut_str, e);
                } else {
                    println!("[shortcuts] Registered stop_scroll -> {}", shortcut_str);
                }
            }
        }
    }
}

/// Unregister stop_scroll shortcuts (call when scroll capture stops)
pub fn unregister_stop_scroll_shortcuts(app: &AppHandle) {
    let config = config::load_config();
    if let Some(shortcuts) = config.shortcuts.get("stop_scroll") {
        for cfg in shortcuts {
            if !cfg.enabled {
                continue;
            }
            let shortcut_str = cfg.to_shortcut_string();
            if let Ok(shortcut) = parse_shortcut(&shortcut_str) {
                if let Err(e) = app.global_shortcut().unregister(shortcut) {
                    eprintln!("[shortcuts] Failed to unregister stop_scroll shortcut ({}): {}", shortcut_str, e);
                } else {
                    println!("[shortcuts] Unregistered stop_scroll -> {}", shortcut_str);
                }
            }
        }
    }
}
