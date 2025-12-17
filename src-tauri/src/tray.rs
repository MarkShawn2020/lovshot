use crate::capture::Screen;
use tauri::image::Image as TauriImage;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::types::Region;

/// Load tray icon
pub fn load_tray_icon(is_recording: bool) -> Option<TauriImage<'static>> {
    let icon_bytes: &[u8] = if is_recording {
        include_bytes!("../icons/tray-recording.png")
    } else {
        include_bytes!("../icons/tray-icon.png")
    };

    let img = image::load_from_memory(icon_bytes).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(TauriImage::new_owned(rgba.into_raw(), width, height))
}

/// Update tray icon (recording state)
pub fn update_tray_icon(app: &AppHandle, is_recording: bool) {
    if let Some(icon) = load_tray_icon(is_recording) {
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_icon(Some(icon));
            let tooltip = if is_recording {
                "Lovshot - Recording... (Option+A to stop)"
            } else {
                "Lovshot - Option+A to capture"
            };
            let _ = tray.set_tooltip(Some(tooltip));
        }
    }
}

/// Create recording border overlay window
pub fn create_recording_overlay(app: &AppHandle, region: &Region, static_mode: bool) {
    if app.get_webview_window("recording-overlay").is_some() {
        return;
    }

    let screens = Screen::all().unwrap_or_default();
    if screens.is_empty() {
        return;
    }

    let screen = &screens[0];
    let scale = screen.display_info.scale_factor;
    let screen_x = screen.display_info.x;
    let screen_y = screen.display_info.y;
    let width = screen.display_info.width;
    let height = screen.display_info.height;

    let mut url = format!(
        "/overlay.html?x={}&y={}&w={}&h={}",
        region.x, region.y, region.width, region.height
    );
    if static_mode {
        url.push_str("&static=1");
    }

    let win = WebviewWindowBuilder::new(app, "recording-overlay", WebviewUrl::App(url.into()))
        .title("Recording Overlay")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(false)
        .focused(false)
        .build();

    if let Ok(win) = win {
        let physical_width = (width as f32 * scale) as u32;
        let physical_height = (height as f32 * scale) as u32;
        let physical_x = (screen_x as f32 * scale) as i32;
        let physical_y = (screen_y as f32 * scale) as i32;

        let _ = win.set_size(PhysicalSize::new(physical_width, physical_height));
        let _ = win.set_position(PhysicalPosition::new(physical_x, physical_y));
        let _ = win.set_ignore_cursor_events(true);

        #[cfg(target_os = "macos")]
        {
            use objc::{msg_send, sel, sel_impl};
            let _ = win.with_webview(|webview| unsafe {
                let ns_window = webview.ns_window() as *mut objc::runtime::Object;
                let _: () = msg_send![ns_window, setLevel: 1000_i64];
            });
        }
    }
}
