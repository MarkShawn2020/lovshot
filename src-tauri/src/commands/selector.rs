use crate::capture::Screen;
use mouse_position::mouse_position::Mouse;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::state::SharedState;
use crate::types::{CaptureMode, Region};
use crate::windows::set_activation_policy;

#[cfg(target_os = "macos")]
use crate::window_detect;

#[tauri::command]
pub fn open_selector(app: AppHandle, state: tauri::State<SharedState>) -> Result<(), String> {
    println!("[DEBUG][open_selector] 入口");

    if let Some(win) = app.get_webview_window("selector") {
        println!("[DEBUG][open_selector] selector 窗口已存在，跳过");
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    // Only hide main window if we're starting a GIF/Video recording (not for screenshots)
    // Screenshots should not disrupt the dashboard view
    let s = state.lock().unwrap();
    let has_frames = !s.frames.is_empty();
    let pending_mode = s.pending_mode;
    drop(s);

    let should_hide = !has_frames
        && matches!(
            pending_mode,
            Some(CaptureMode::Gif) | Some(CaptureMode::Video)
        );
    if should_hide {
        if let Some(main_win) = app.get_webview_window("main") {
            println!("[DEBUG][open_selector] GIF/Video 模式，隐藏主窗口");
            let _ = main_win.hide();
            set_activation_policy(1);
        }
    } else {
        println!("[DEBUG][open_selector] 截图/滚动模式或有编辑数据，保持主窗口");
    }

    let screens = Screen::all().map_err(|e| e.to_string())?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let screen = &screens[0];
    let screen_x = screen.display_info.x;
    let screen_y = screen.display_info.y;
    let width = screen.display_info.width;
    let height = screen.display_info.height;
    let scale = screen.display_info.scale_factor;

    {
        let mut s = state.lock().unwrap();
        s.screen_x = screen_x;
        s.screen_y = screen_y;
        s.screen_scale = scale;
    }

    println!("[DEBUG][open_selector] 准备创建 selector 窗口");

    let win = WebviewWindowBuilder::new(&app, "selector", WebviewUrl::App("/selector.html".into()))
        .title("Select Region")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .accept_first_mouse(true)
        .build()
        .map_err(|e| e.to_string())?;

    let physical_width = (width as f32 * scale) as u32;
    let physical_height = (height as f32 * scale) as u32;
    let physical_x = (screen_x as f32 * scale) as i32;
    let physical_y = (screen_y as f32 * scale) as i32;

    win.set_size(PhysicalSize::new(physical_width, physical_height))
        .map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(physical_x, physical_y))
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    {
        use objc::{class, msg_send, sel, sel_impl};
        let _ = win.with_webview(|webview| {
            unsafe {
                let ns_window = webview.ns_window() as *mut objc::runtime::Object;
                let _: () = msg_send![ns_window, setLevel: 1000_i64];
                // Prevent mouse events from passing through transparent areas
                let _: () = msg_send![ns_window, setIgnoresMouseEvents: false];
                // Set a minimal background color to capture mouse events
                let ns_color_class = class!(NSColor);
                let clear_color: *mut objc::runtime::Object =
                    msg_send![ns_color_class, colorWithWhite:0.0_f64 alpha:0.005_f64];
                let _: () = msg_send![ns_window, setBackgroundColor: clear_color];
            }
        });
    }

    Ok(())
}

#[tauri::command]
pub fn set_region(state: tauri::State<SharedState>, region: Region) {
    println!(
        "[DEBUG][set_region] ====== 被调用 ====== x={}, y={}, w={}, h={}",
        region.x, region.y, region.width, region.height
    );
    let mut s = state.lock().unwrap();
    println!("[DEBUG][set_region] 直接使用逻辑像素坐标（不缩放）");
    s.region = Some(region);
}

#[tauri::command]
pub fn get_pending_mode(state: tauri::State<SharedState>) -> Option<CaptureMode> {
    let mode = state.lock().unwrap().pending_mode;
    println!("[DEBUG][get_pending_mode] 返回: {:?}", mode);
    mode
}

#[tauri::command]
pub fn get_screen_snapshot(state: tauri::State<SharedState>) -> Option<String> {
    state.lock().unwrap().screen_snapshot.clone()
}

#[tauri::command]
pub fn get_window_at_cursor() -> Option<Region> {
    #[cfg(target_os = "macos")]
    {
        if let Mouse::Position { x, y } = Mouse::get_mouse_position() {
            return window_detect::get_window_at_position(x as f64, y as f64);
        }
    }
    None
}

/// Get window info at cursor including titlebar height (for exclude-titlebar feature)
#[tauri::command]
pub fn get_window_info_at_cursor() -> Option<window_detect::WindowInfo> {
    #[cfg(target_os = "macos")]
    {
        if let Mouse::Position { x, y } = Mouse::get_mouse_position() {
            return window_detect::get_window_info_at_position(x as f64, y as f64);
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    None
}

#[tauri::command]
pub fn clear_pending_mode(state: tauri::State<SharedState>) {
    state.lock().unwrap().pending_mode = None;
}

/// Activate the window under cursor so it can receive scroll events
#[tauri::command]
pub fn activate_window_under_cursor() -> bool {
    #[cfg(target_os = "macos")]
    {
        if let Mouse::Position { x, y } = Mouse::get_mouse_position() {
            return window_detect::activate_window_at_position(x as f64, y as f64);
        }
    }
    false
}

/// Internal function to open selector (called from shortcut handler)
pub fn open_selector_internal(app: AppHandle) -> Result<(), String> {
    println!("[DEBUG][open_selector_internal] 入口");

    if let Some(win) = app.get_webview_window("selector") {
        println!("[DEBUG][open_selector_internal] selector 窗口已存在，跳过");
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    // Only hide main window if we're starting a GIF/Video recording (not for screenshots)
    let state = app.state::<SharedState>();
    let s = state.lock().unwrap();
    let has_frames = !s.frames.is_empty();
    let pending_mode = s.pending_mode;
    drop(s);

    let should_hide = !has_frames
        && matches!(
            pending_mode,
            Some(CaptureMode::Gif) | Some(CaptureMode::Video)
        );
    if should_hide {
        if let Some(main_win) = app.get_webview_window("main") {
            let _ = main_win.hide();
            set_activation_policy(1);
        }
    }

    let screens = Screen::all().map_err(|e| e.to_string())?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let screen = &screens[0];
    let screen_x = screen.display_info.x;
    let screen_y = screen.display_info.y;
    let width = screen.display_info.width;
    let height = screen.display_info.height;
    let scale = screen.display_info.scale_factor;

    {
        let state = app.state::<SharedState>();
        let mut s = state.lock().unwrap();
        s.screen_x = screen_x;
        s.screen_y = screen_y;
        s.screen_scale = scale;
    }

    let win = WebviewWindowBuilder::new(&app, "selector", WebviewUrl::App("/selector.html".into()))
        .title("Select Region")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .accept_first_mouse(true)
        .build()
        .map_err(|e| e.to_string())?;

    let physical_width = (width as f32 * scale) as u32;
    let physical_height = (height as f32 * scale) as u32;
    let physical_x = (screen_x as f32 * scale) as i32;
    let physical_y = (screen_y as f32 * scale) as i32;

    win.set_size(PhysicalSize::new(physical_width, physical_height))
        .map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(physical_x, physical_y))
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    {
        use objc::{msg_send, sel, sel_impl};
        let _ = win.with_webview(|webview| unsafe {
            let ns_window = webview.ns_window() as *mut objc::runtime::Object;
            let _: () = msg_send![ns_window, setLevel: 1000_i64];
        });
    }

    Ok(())
}
