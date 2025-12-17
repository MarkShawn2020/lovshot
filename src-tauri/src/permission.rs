//! macOS Screen Recording Permission Check
//!
//! Uses CGPreflightScreenCaptureAccess() and CGRequestScreenCaptureAccess()
//! to check and request screen recording permission.

#[cfg(target_os = "macos")]
use core_graphics::access::ScreenCaptureAccess;

/// Check if screen recording permission is granted (without prompting)
#[cfg(target_os = "macos")]
pub fn has_screen_recording_permission() -> bool {
    let access = ScreenCaptureAccess;
    access.preflight()
}

/// Request screen recording permission (will prompt user if not yet decided)
/// Returns true if permission was granted, false otherwise
#[cfg(target_os = "macos")]
pub fn request_screen_recording_permission() -> bool {
    let access = ScreenCaptureAccess;
    access.request()
}

/// Open System Preferences to Screen Recording settings
#[cfg(target_os = "macos")]
pub fn open_screen_recording_settings() -> Result<(), String> {
    use std::process::Command;

    // macOS 13+ uses Privacy & Security, older versions use Security & Privacy
    let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

    Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| format!("Failed to open System Preferences: {}", e))?;

    Ok(())
}

// Non-macOS stubs
#[cfg(not(target_os = "macos"))]
pub fn has_screen_recording_permission() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
pub fn request_screen_recording_permission() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
pub fn open_screen_recording_settings() -> Result<(), String> {
    Ok(())
}
