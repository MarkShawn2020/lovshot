use crate::permission;

#[derive(serde::Serialize)]
pub struct PermissionStatus {
    pub granted: bool,
}

/// Check if screen recording permission is granted
#[tauri::command]
pub fn check_screen_permission() -> PermissionStatus {
    PermissionStatus {
        granted: permission::has_screen_recording_permission(),
    }
}

/// Request screen recording permission (triggers system dialog if needed)
#[tauri::command]
pub fn request_screen_permission() -> PermissionStatus {
    PermissionStatus {
        granted: permission::request_screen_recording_permission(),
    }
}

/// Open System Preferences to Screen Recording settings
#[tauri::command]
pub fn open_permission_settings() -> Result<(), String> {
    permission::open_screen_recording_settings()
}
