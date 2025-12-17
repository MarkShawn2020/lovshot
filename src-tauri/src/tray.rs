use crate::capture::Screen;
use crate::config;
use tauri::image::Image as TauriImage;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::types::Region;

/// Build tray menu with current shortcuts from config
pub fn build_tray_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let cfg = config::load_config();
    let screenshot_shortcut = cfg
        .shortcuts
        .get("screenshot")
        .map(|s| s.to_shortcut_string())
        .unwrap_or_else(|| "Alt+A".to_string());
    let gif_shortcut = cfg
        .shortcuts
        .get("gif")
        .map(|s| s.to_shortcut_string())
        .unwrap_or_else(|| "Alt+G".to_string());
    let video_shortcut = cfg
        .shortcuts
        .get("video")
        .map(|s| s.to_shortcut_string())
        .unwrap_or_else(|| "Alt+V".to_string());
    let scroll_shortcut = cfg
        .shortcuts
        .get("scroll")
        .map(|s| s.to_shortcut_string())
        .unwrap_or_else(|| "Alt+S".to_string());

    let menu_show = MenuItem::with_id(app, "show", "Show Lovshot", true, None::<&str>)?;
    let menu_sep0 = PredefinedMenuItem::separator(app)?;
    let menu_screenshot = MenuItem::with_id(
        app,
        "screenshot",
        "Screenshot",
        true,
        Some(screenshot_shortcut.as_str()),
    )?;
    let menu_gif =
        MenuItem::with_id(app, "gif", "Record GIF", true, Some(gif_shortcut.as_str()))?;
    let menu_scroll = MenuItem::with_id(
        app,
        "scroll",
        "Scroll Capture",
        cfg.developer_mode,
        Some(scroll_shortcut.as_str()),
    )?;
    let menu_video = MenuItem::with_id(
        app,
        "video",
        "Record Video",
        false,
        Some(video_shortcut.as_str()),
    )?;
    let menu_sep1 = PredefinedMenuItem::separator(app)?;
    let menu_settings = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
    let menu_sep2 = PredefinedMenuItem::separator(app)?;
    let menu_about = MenuItem::with_id(app, "about", "About Lovshot", true, None::<&str>)?;
    let menu_sep3 = PredefinedMenuItem::separator(app)?;
    let menu_quit = MenuItem::with_id(app, "quit", "Quit Lovshot", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &menu_show,
            &menu_sep0,
            &menu_screenshot,
            &menu_gif,
            &menu_scroll,
            &menu_video,
            &menu_sep1,
            &menu_settings,
            &menu_sep2,
            &menu_about,
            &menu_sep3,
            &menu_quit,
        ],
    )
}

/// Update tray menu with current config (call after shortcut changes)
pub fn update_tray_menu(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = build_tray_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

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
