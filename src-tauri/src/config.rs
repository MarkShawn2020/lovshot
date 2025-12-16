use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Shortcut configuration for a single action
#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// Application configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: String,
    pub shortcuts: HashMap<String, ShortcutConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut shortcuts = HashMap::new();

        shortcuts.insert(
            "screenshot".to_string(),
            ShortcutConfig {
                modifiers: vec!["Alt".to_string()],
                key: "A".to_string(),
                enabled: true,
            },
        );

        shortcuts.insert(
            "gif".to_string(),
            ShortcutConfig {
                modifiers: vec!["Alt".to_string()],
                key: "G".to_string(),
                enabled: true,
            },
        );

        shortcuts.insert(
            "video".to_string(),
            ShortcutConfig {
                modifiers: vec!["Alt".to_string()],
                key: "V".to_string(),
                enabled: true,
            },
        );

        Self {
            version: "1.0.0".to_string(),
            shortcuts,
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

/// Load configuration from file, or return default if not exists
pub fn load_config() -> AppConfig {
    let path = get_config_path();

    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => return config,
                Err(e) => {
                    eprintln!("[config] Failed to parse config: {}", e);
                }
            },
            Err(e) => {
                eprintln!("[config] Failed to read config file: {}", e);
            }
        }
    }

    // Return default and save it
    let config = AppConfig::default();
    let _ = save_config(&config);
    config
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

/// Update a single shortcut in the config
pub fn update_shortcut(action: &str, shortcut: ShortcutConfig) -> Result<AppConfig, String> {
    let mut config = load_config();
    config.shortcuts.insert(action.to_string(), shortcut);
    save_config(&config)?;
    Ok(config)
}
