use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Shortcut configuration for a single shortcut binding
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ShortcutConfig {
    pub modifiers: Vec<String>, // ["Alt"], ["Ctrl", "Shift"], etc.
    pub key: String,            // "A", "G", "V", etc.
    pub enabled: bool,
}

impl ShortcutConfig {
    /// Convert to shortcut string format: "Alt+A", "Ctrl+Shift+K"
    pub fn to_shortcut_string(&self) -> String {
        if self.modifiers.is_empty() {
            self.key.clone()
        } else {
            format!("{}+{}", self.modifiers.join("+"), self.key)
        }
    }

    /// Parse from shortcut string format
    pub fn from_shortcut_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('+').collect();
        if parts.is_empty() {
            return None;
        }
        let key = parts.last()?.to_string();
        let modifiers: Vec<String> = parts[..parts.len() - 1]
            .iter()
            .map(|s| s.to_string())
            .collect();
        Some(Self {
            modifiers,
            key,
            enabled: true,
        })
    }
}

/// Application configuration (v2 - supports multiple shortcuts per action)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: String,
    pub shortcuts: HashMap<String, Vec<ShortcutConfig>>,
    #[serde(default)]
    pub developer_mode: bool,
    #[serde(default = "default_autostart")]
    pub autostart_enabled: bool,
    #[serde(default)]
    pub scroll_capture_enabled: bool,
}

/// Old config format for migration
#[derive(Clone, Debug, Deserialize)]
struct OldAppConfig {
    pub version: String,
    pub shortcuts: HashMap<String, ShortcutConfig>,
    #[serde(default)]
    pub developer_mode: bool,
    #[serde(default = "default_autostart")]
    pub autostart_enabled: bool,
    #[serde(default)]
    pub scroll_capture_enabled: bool,
}

impl From<OldAppConfig> for AppConfig {
    fn from(old: OldAppConfig) -> Self {
        let shortcuts = old
            .shortcuts
            .into_iter()
            .map(|(k, v)| (k, vec![v]))
            .collect();
        Self {
            version: "2.0.0".to_string(),
            shortcuts,
            developer_mode: old.developer_mode,
            autostart_enabled: old.autostart_enabled,
            scroll_capture_enabled: old.scroll_capture_enabled,
        }
    }
}

fn default_autostart() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut shortcuts = HashMap::new();

        // Static screenshot (frozen screen) - default Alt+A
        shortcuts.insert(
            "screenshot_static".to_string(),
            vec![ShortcutConfig {
                modifiers: vec!["Alt".to_string()],
                key: "A".to_string(),
                enabled: true,
            }],
        );

        // Dynamic screenshot (live screen) - Shift+Alt+A
        shortcuts.insert(
            "screenshot".to_string(),
            vec![ShortcutConfig {
                modifiers: vec!["Shift".to_string(), "Alt".to_string()],
                key: "A".to_string(),
                enabled: true,
            }],
        );

        shortcuts.insert(
            "gif".to_string(),
            vec![ShortcutConfig {
                modifiers: vec!["Alt".to_string()],
                key: "G".to_string(),
                enabled: true,
            }],
        );

        shortcuts.insert(
            "video".to_string(),
            vec![ShortcutConfig {
                modifiers: vec!["Alt".to_string()],
                key: "V".to_string(),
                enabled: true,
            }],
        );

        shortcuts.insert(
            "scroll".to_string(),
            vec![ShortcutConfig {
                modifiers: vec!["Alt".to_string()],
                key: "S".to_string(),
                enabled: true,
            }],
        );

        // Note: The shortcut used to START recording (e.g., Alt+G) automatically
        // stops recording when pressed again. This config is for ADDITIONAL stop keys.
        shortcuts.insert(
            "stop_recording".to_string(),
            vec![ShortcutConfig {
                modifiers: vec![],
                key: "Escape".to_string(),
                enabled: true,
            }],
        );

        shortcuts.insert(
            "stop_scroll".to_string(),
            vec![ShortcutConfig {
                modifiers: vec![],
                key: "Escape".to_string(),
                enabled: true,
            }],
        );

        shortcuts.insert(
            "show_main".to_string(),
            vec![ShortcutConfig {
                modifiers: vec!["Alt".to_string()],
                key: "O".to_string(),
                enabled: true,
            }],
        );

        Self {
            version: "2.0.0".to_string(),
            shortcuts,
            developer_mode: false,
            autostart_enabled: true,
            scroll_capture_enabled: false,
        }
    }
}

/// Get the config file path
pub fn get_config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));

    config_dir.join("lovshot").join("config.json")
}

/// Check if config is in old format (shortcuts are objects, not arrays)
fn is_old_format(content: &str) -> bool {
    if let Ok(v) = serde_json::from_str::<Value>(content) {
        if let Some(shortcuts) = v.get("shortcuts") {
            if let Some(obj) = shortcuts.as_object() {
                // Check first shortcut value - if it's an object (not array), it's old format
                if let Some((_, first_val)) = obj.iter().next() {
                    return first_val.is_object();
                }
            }
        }
    }
    false
}

/// Ensure stop_recording has all default shortcuts (merge missing ones)
fn ensure_stop_recording_defaults(config: &mut AppConfig, default_config: &AppConfig) -> bool {
    let mut updated = false;
    if let Some(default_shortcuts) = default_config.shortcuts.get("stop_recording") {
        let current = config
            .shortcuts
            .entry("stop_recording".to_string())
            .or_insert_with(Vec::new);

        for default_sc in default_shortcuts {
            let exists = current.iter().any(|sc| {
                sc.modifiers == default_sc.modifiers && sc.key == default_sc.key
            });
            if !exists {
                println!(
                    "[config] Adding missing stop_recording shortcut: {}",
                    default_sc.to_shortcut_string()
                );
                current.push(default_sc.clone());
                updated = true;
            }
        }
    }
    updated
}

/// Load configuration from file, or return default if not exists
/// Also ensures any missing shortcuts from default config are added
/// Handles migration from v1 (single shortcut) to v2 (multiple shortcuts)
pub fn load_config() -> AppConfig {
    let path = get_config_path();
    let default_config = AppConfig::default();

    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => {
                // Check if old format and migrate
                if is_old_format(&content) {
                    println!("[config] Detected old format, migrating to v2...");
                    match serde_json::from_str::<OldAppConfig>(&content) {
                        Ok(old_config) => {
                            let mut config: AppConfig = old_config.into();
                            // Add any missing shortcuts
                            for (key, value) in &default_config.shortcuts {
                                if !config.shortcuts.contains_key(key) {
                                    println!("[config] Adding missing shortcut: {}", key);
                                    config.shortcuts.insert(key.clone(), value.clone());
                                }
                            }
                            // Ensure stop_recording has all defaults
                            ensure_stop_recording_defaults(&mut config, &default_config);
                            let _ = save_config(&config);
                            return config;
                        }
                        Err(e) => {
                            eprintln!("[config] Failed to parse old config: {}", e);
                        }
                    }
                } else {
                    // Try parsing as new format
                    match serde_json::from_str::<AppConfig>(&content) {
                        Ok(mut config) => {
                            // Add any missing shortcuts from default config
                            let mut updated = false;
                            for (key, value) in &default_config.shortcuts {
                                if !config.shortcuts.contains_key(key) {
                                    println!("[config] Adding missing shortcut: {}", key);
                                    config.shortcuts.insert(key.clone(), value.clone());
                                    updated = true;
                                }
                            }
                            // Ensure stop_recording has all defaults
                            if ensure_stop_recording_defaults(&mut config, &default_config) {
                                updated = true;
                            }
                            if updated {
                                let _ = save_config(&config);
                            }
                            return config;
                        }
                        Err(e) => {
                            eprintln!("[config] Failed to parse config: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[config] Failed to read config file: {}", e);
            }
        }
    }

    // Return default and save it
    let _ = save_config(&default_config);
    default_config
}

/// Save configuration to file
pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;

    println!("[config] Saved to {:?}", path);
    Ok(())
}

/// Update shortcuts for an action (replaces all shortcuts for that action)
pub fn update_shortcuts(action: &str, shortcuts: Vec<ShortcutConfig>) -> Result<AppConfig, String> {
    let mut config = load_config();
    config.shortcuts.insert(action.to_string(), shortcuts);
    save_config(&config)?;
    Ok(config)
}

/// Add a shortcut to an action
pub fn add_shortcut(action: &str, shortcut: ShortcutConfig) -> Result<AppConfig, String> {
    let mut config = load_config();
    let shortcuts = config.shortcuts.entry(action.to_string()).or_insert_with(Vec::new);
    // Avoid duplicates
    if !shortcuts.iter().any(|s| s.modifiers == shortcut.modifiers && s.key == shortcut.key) {
        shortcuts.push(shortcut);
    }
    save_config(&config)?;
    Ok(config)
}

/// Remove a shortcut from an action by index
pub fn remove_shortcut(action: &str, index: usize) -> Result<AppConfig, String> {
    let mut config = load_config();
    if let Some(shortcuts) = config.shortcuts.get_mut(action) {
        if index < shortcuts.len() && shortcuts.len() > 1 {
            shortcuts.remove(index);
        }
    }
    save_config(&config)?;
    Ok(config)
}
