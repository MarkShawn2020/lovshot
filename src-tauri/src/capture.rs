//! Screen capture abstraction using xcap
//!
//! Provides a unified API for screen capture operations.

use image::RgbaImage;
use xcap::Monitor;

/// Display information matching the old screenshots API
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

/// Screen wrapper providing capture operations
pub struct Screen {
    monitor: Monitor,
    pub display_info: DisplayInfo,
}

impl Screen {
    /// Get all available screens
    pub fn all() -> Result<Vec<Screen>, String> {
        let monitors = Monitor::all().map_err(|e| e.to_string())?;

        monitors
            .into_iter()
            .enumerate()
            .map(|(idx, monitor)| {
                // xcap Monitor API - width/height return Result
                let width = monitor.width().map_err(|e| e.to_string())?;
                let height = monitor.height().map_err(|e| e.to_string())?;

                // xcap may not expose x/y directly, default to 0 for primary
                // For multi-monitor setups, this would need platform-specific code
                let (x, y) = get_monitor_position(&monitor, idx);
                let scale_factor = get_scale_factor(&monitor, width);

                Ok(Screen {
                    display_info: DisplayInfo {
                        id: idx as u32,
                        x,
                        y,
                        width,
                        height,
                        scale_factor,
                    },
                    monitor,
                })
            })
            .collect()
    }

    /// Capture entire screen
    pub fn capture(&self) -> Result<RgbaImage, String> {
        let img = self.monitor.capture_image().map_err(|e| e.to_string())?;
        Ok(img)
    }

    /// Capture a specific area of the screen
    /// Note: x, y, width, height are in logical pixels (CSS pixels)
    /// xcap returns physical pixels, so we scale by scale_factor
    pub fn capture_area(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, String> {
        // xcap's capture_image returns the full monitor in physical pixels
        let full = self.monitor.capture_image().map_err(|e| e.to_string())?;

        let scale = self.display_info.scale_factor;

        // Convert logical pixels to physical pixels
        let rel_x = ((x - self.display_info.x) as f32 * scale).max(0.0) as u32;
        let rel_y = ((y - self.display_info.y) as f32 * scale).max(0.0) as u32;
        let phys_w = (width as f32 * scale) as u32;
        let phys_h = (height as f32 * scale) as u32;

        // Clamp to valid bounds
        let max_x = full.width().saturating_sub(1);
        let max_y = full.height().saturating_sub(1);
        let crop_x = rel_x.min(max_x);
        let crop_y = rel_y.min(max_y);
        let crop_w = phys_w.min(full.width().saturating_sub(crop_x));
        let crop_h = phys_h.min(full.height().saturating_sub(crop_y));

        if crop_w == 0 || crop_h == 0 {
            return Err("Invalid capture area".to_string());
        }

        let cropped = image::imageops::crop_imm(&full, crop_x, crop_y, crop_w, crop_h).to_image();
        Ok(cropped)
    }
}

/// Get monitor position (platform-specific)
#[cfg(target_os = "macos")]
fn get_monitor_position(monitor: &Monitor, _idx: usize) -> (i32, i32) {
    // On macOS, try to get position from monitor
    // xcap 0.7 may expose this through name() or other means
    // For now, use CoreGraphics to get accurate position
    use core_graphics::display::CGDisplay;

    let displays = CGDisplay::active_displays().unwrap_or_default();
    if let Some(&display_id) = displays.get(_idx) {
        let display = CGDisplay::new(display_id);
        let bounds = display.bounds();
        return (bounds.origin.x as i32, bounds.origin.y as i32);
    }
    (0, 0)
}

#[cfg(not(target_os = "macos"))]
fn get_monitor_position(_monitor: &Monitor, _idx: usize) -> (i32, i32) {
    (0, 0)
}

/// Get scale factor (platform-specific)
#[cfg(target_os = "macos")]
fn get_scale_factor(_monitor: &Monitor, logical_width: u32) -> f32 {
    // Use CoreGraphics to get pixel dimensions
    use core_graphics::display::CGDisplay;

    let main = CGDisplay::main();
    if let Some(mode) = main.display_mode() {
        let pixel_width = mode.pixel_width() as f32;
        let logical = logical_width as f32;

        if logical > 0.0 {
            return (pixel_width / logical).max(1.0);
        }
    }
    2.0 // Default Retina scale
}

#[cfg(not(target_os = "macos"))]
fn get_scale_factor(_monitor: &Monitor, _logical_width: u32) -> f32 {
    1.0
}
