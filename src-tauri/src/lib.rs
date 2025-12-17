use std::sync::{Arc, Mutex};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

#[cfg(target_os = "macos")]
mod macos_menu_tracking;
#[cfg(target_os = "macos")]
mod window_detect;

mod capture;
mod commands;
mod config;
mod fft_match;
mod shortcuts;
mod state;
mod tray;
mod types;
mod windows;

use commands::open_selector_internal;
use shortcuts::{format_shortcut_display, get_action_for_shortcut, register_shortcuts_from_config};
use state::{AppState, SharedState};
use tray::load_tray_icon;
pub use types::*;
use windows::{open_about_window, open_settings_window};

#[tauri::command]
fn show_main_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
        windows::set_activation_policy(0); // Regular app mode
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state: SharedState = Arc::new(Mutex::new(AppState::default()));

    let state_for_shortcut = state.clone();
    let state_for_tray = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }

                    let is_recording = state_for_shortcut.lock().unwrap().recording;
                    if is_recording {
                        println!("[DEBUG][shortcut] 停止录制");
                        state_for_shortcut.lock().unwrap().recording = false;
                        return;
                    }

                    // Check if scroll capturing - if so, stop and allow new captures
                    {
                        let mut s = state_for_shortcut.lock().unwrap();
                        if s.scroll_capturing {
                            println!("[DEBUG][shortcut] 停止滚动截图");
                            s.scroll_capturing = false;
                            drop(s);
                            let _ = app.emit("scroll-capture-stop", ());
                            return;
                        }
                    }

                    if let Some(mode) = get_action_for_shortcut(shortcut) {
                        println!("[DEBUG][shortcut] {:?} triggered -> {:?}", shortcut, mode);
                        state_for_shortcut.lock().unwrap().pending_mode = Some(mode);
                        let _ = open_selector_internal(app.clone());
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_screens,
            commands::get_mouse_position,
            commands::capture_screenshot,
            commands::open_selector,
            commands::set_region,
            commands::get_pending_mode,
            commands::get_screen_snapshot,
            commands::clear_pending_mode,
            commands::get_window_at_cursor,
            commands::get_window_info_at_cursor,
            commands::get_shortcuts_config,
            commands::save_shortcut,
            commands::reset_shortcuts_to_default,
            commands::pause_shortcuts,
            commands::resume_shortcuts,
            commands::start_recording,
            commands::stop_recording,
            commands::get_recording_info,
            commands::estimate_export_size,
            commands::export_gif,
            commands::discard_recording,
            commands::get_frame_thumbnail,
            commands::get_filmstrip,
            commands::save_screenshot,
            commands::open_file,
            commands::reveal_in_folder,
            // Scroll capture commands
            commands::start_scroll_capture,
            commands::capture_scroll_frame_auto,
            commands::get_scroll_preview,
            commands::copy_scroll_to_clipboard,
            commands::finish_scroll_capture,
            commands::stop_scroll_capture,
            commands::cancel_scroll_capture,
            commands::open_scroll_overlay,
            commands::get_history,
            commands::get_stats,
            show_main_window,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    window.hide().unwrap();
                    // Switch back to Accessory policy when hiding main window
                    windows::set_activation_policy(1);
                    api.prevent_close();
                }
            }
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            {
                use objc::{class, msg_send, sel, sel_impl};
                unsafe {
                    let app_class = class!(NSApplication);
                    let ns_app: *mut objc::runtime::Object =
                        msg_send![app_class, sharedApplication];
                    let _: () = msg_send![ns_app, setActivationPolicy: 1_i64];
                }
            }

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
                true,
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
            let menu_settings =
                MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let menu_sep2 = PredefinedMenuItem::separator(app)?;
            let menu_about = MenuItem::with_id(app, "about", "About Lovshot", true, None::<&str>)?;
            let menu_sep3 = PredefinedMenuItem::separator(app)?;
            let menu_quit = MenuItem::with_id(app, "quit", "Quit Lovshot", true, None::<&str>)?;

            let tray_menu = Menu::with_items(
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
            )?;

            let tray_icon =
                load_tray_icon(false).unwrap_or_else(|| app.default_window_icon().unwrap().clone());

            let state_for_menu = state_for_tray.clone();
            #[cfg(target_os = "macos")]
            {
                macos_menu_tracking::install_menu_tracking_observers(
                    app.handle(),
                    state_for_tray.clone(),
                );
            }
            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("Lovshot")
                .menu(&tray_menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                            windows::set_activation_policy(0);
                        }
                    }
                    "screenshot" => {
                        state_for_menu.lock().unwrap().pending_mode = Some(CaptureMode::Image);
                        let _ = open_selector_internal(app.clone());
                    }
                    "gif" => {
                        state_for_menu.lock().unwrap().pending_mode = Some(CaptureMode::Gif);
                        let _ = open_selector_internal(app.clone());
                    }
                    "scroll" => {
                        state_for_menu.lock().unwrap().pending_mode = Some(CaptureMode::Scroll);
                        let _ = open_selector_internal(app.clone());
                    }
                    "video" => {
                        state_for_menu.lock().unwrap().pending_mode = Some(CaptureMode::Video);
                        let _ = open_selector_internal(app.clone());
                    }
                    "settings" => {
                        let _ = open_settings_window(app.clone());
                    }
                    "about" => {
                        let _ = open_about_window(app.clone());
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .menu_on_left_click(true)
                .build(app)?;

            let app_handle = app.handle().clone();
            register_shortcuts_from_config(&app_handle)?;

            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.hide();
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
