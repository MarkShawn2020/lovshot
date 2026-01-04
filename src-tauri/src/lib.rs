use std::sync::{Arc, Mutex};

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::ShortcutState;

#[cfg(target_os = "macos")]
mod macos_menu_tracking;
#[cfg(target_os = "macos")]
mod native_screenshot;
#[cfg(target_os = "macos")]
mod scroll_event;
#[cfg(target_os = "macos")]
mod window_detect;

mod capture;
mod commands;
mod config;
mod fft_match;
mod row_hash;
mod permission;
mod share_compose;
mod shortcuts;
mod state;
mod tray;
mod types;
mod windows;

use commands::open_selector_internal;
use shortcuts::{get_action_for_shortcut, is_show_main_shortcut, is_stop_recording_shortcut, register_shortcuts_from_config, unregister_stop_shortcuts, unregister_stop_scroll_shortcuts};
use state::{AppState, SharedState};
use tray::{build_tray_menu, load_tray_icon};
pub use types::*;
use windows::{open_about_window, open_permission_window, open_settings_window};

#[tauri::command]
fn show_main_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
        windows::set_activation_policy(0); // Regular app mode
    }
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state: SharedState = Arc::new(Mutex::new(AppState::default()));

    let state_for_shortcut = state.clone();
    let state_for_tray = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::AppleScript,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }

                    // If recording, any registered shortcut stops it
                    let is_recording = state_for_shortcut.lock().unwrap().recording;
                    if is_recording {
                        println!("[DEBUG][shortcut] 停止录制");
                        state_for_shortcut.lock().unwrap().recording = false;
                        // IMPORTANT: Unregister in spawned thread to avoid deadlock
                        let app_clone = app.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                            unregister_stop_shortcuts(&app_clone);
                        });
                        return;
                    }

                    // Check if scroll-overlay window exists - if so, close it directly
                    // This is more reliable than depending on frontend event listeners
                    {
                        let scroll_overlay_exists = app.get_webview_window("scroll-overlay").is_some();

                        if scroll_overlay_exists {
                            println!("[DEBUG][shortcut] 检测到滚动截图窗口，直接关闭");

                            // Try to clean up state (non-blocking)
                            if let Ok(mut s) = state_for_shortcut.try_lock() {
                                s.scroll_capturing = false;
                                s.scroll_frames.clear();
                                s.scroll_offsets.clear();
                                s.scroll_stitched = None;
                            }
                            #[cfg(target_os = "macos")]
                            crate::scroll_event::stop_scroll_listener();

                            // IMPORTANT: Do NOT call unregister() here - it causes deadlock!
                            // The shortcut handler callback cannot call unregister() on itself.
                            // Instead, unregister in a spawned thread after returning.
                            let app_clone = app.clone();
                            std::thread::spawn(move || {
                                // Small delay to ensure handler has returned
                                std::thread::sleep(std::time::Duration::from_millis(10));
                                crate::shortcuts::unregister_stop_scroll_shortcuts(&app_clone);
                            });

                            // Close windows directly from backend
                            if let Some(win) = app.get_webview_window("scroll-overlay") {
                                let _ = win.destroy();
                            }
                            if let Some(win) = app.get_webview_window("recording-overlay") {
                                let _ = win.destroy();
                            }

                            println!("[DEBUG][shortcut] 滚动截图窗口已关闭");
                            return;
                        }
                    }

                    // Check if this is a stop/cancel shortcut (ESC, etc.)
                    if is_stop_recording_shortcut(shortcut) {
                        // Close selector window if open
                        if let Some(selector_win) = app.get_webview_window("selector") {
                            if selector_win.is_visible().unwrap_or(false) {
                                println!("[DEBUG][shortcut] 关闭选择器");
                                let _ = selector_win.close();
                            }
                        }
                        return;
                    }

                    // Check if this is show_main shortcut (Alt+O)
                    if is_show_main_shortcut(shortcut) {
                        println!("[DEBUG][shortcut] 打开主窗口");
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                            windows::set_activation_policy(0);
                        }
                        return;
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
            commands::get_magnifier_snapshot,
            commands::get_magnifier_pixels,
            commands::clear_pending_mode,
            commands::capture_screen_now,
            commands::clear_screen_background,
            commands::get_window_at_cursor,
            commands::get_window_info_at_cursor,
            commands::get_shortcuts_config,
            commands::save_shortcut,
            commands::add_shortcut,
            commands::remove_shortcut,
            commands::reset_shortcuts_to_default,
            commands::pause_shortcuts,
            commands::resume_shortcuts,
            commands::set_developer_mode,
            commands::set_scroll_capture_enabled,
            commands::set_screenshot_preview_enabled,
            commands::get_image_export_format,
            commands::set_image_export_format,
            commands::get_screenshot_count,
            commands::get_watermark_position,
            commands::set_watermark_position,
            commands::get_show_caption_editor,
            commands::set_show_caption_editor,
            commands::start_recording,
            commands::stop_recording,
            commands::get_recording_info,
            commands::estimate_export_size,
            commands::export_gif,
            commands::discard_recording,
            commands::get_frame_thumbnail,
            commands::get_filmstrip,
            commands::save_screenshot,
            commands::save_caption,
            commands::get_image_description,
            commands::open_file,
            commands::reveal_in_folder,
            commands::delete_file,
            commands::open_caption_editor,
            commands::open_zoom_viewer,
            commands::copy_image_to_clipboard,
            commands::copy_rgba_to_clipboard,
            commands::save_rgba_to_file,
            // Folder commands
            commands::get_folders,
            commands::create_folder,
            commands::delete_folder,
            commands::rename_folder,
            commands::move_to_folder,
            commands::export_folder_to_md,
            commands::preview_folder_export,
            // Scroll capture commands
            commands::start_scroll_capture,
            commands::capture_scroll_frame_auto,
            commands::get_scroll_preview,
            commands::copy_scroll_to_clipboard,
            commands::finish_scroll_capture,
            commands::stop_scroll_capture,
            commands::cancel_scroll_capture,
            commands::open_scroll_overlay,
            commands::start_scroll_capture_inline,
            commands::get_history,
            commands::get_stats,
            commands::get_autostart_enabled,
            commands::set_autostart_enabled,
            commands::check_screen_permission,
            commands::request_screen_permission,
            commands::open_permission_settings,
            // Annotation commands
            commands::capture_region_preview,
            commands::save_annotated_screenshot,
            // Share compose
            share_compose::compose_share,
            // Debug commands
            commands::open_devtools,
            commands::set_selector_mouse_passthrough,
            commands::setup_scroll_capture_passthrough,
            show_main_window,
            quit_app,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    println!("[DEBUG][close] CloseRequested for main window");
                    println!("[DEBUG][close] Step 1: set_activation_policy(1)");
                    windows::set_activation_policy(1);
                    println!("[DEBUG][close] Step 2: window.hide()");
                    window.hide().unwrap();
                    println!("[DEBUG][close] Step 3: prevent_close");
                    api.prevent_close();
                    println!("[DEBUG][close] Done");
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

            let tray_menu = build_tray_menu(app.handle())?;

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
                    "devtools" => {
                        // Toggle devtools - prioritize selector, fallback to main (and show it)
                        if let Some(win) = app.get_webview_window("selector") {
                            println!("[devtools] Opening devtools for selector");
                            win.open_devtools();
                        } else if let Some(win) = app.get_webview_window("main") {
                            println!("[devtools] Opening devtools for main (showing window first)");
                            let _ = win.show();
                            let _ = win.set_focus();
                            windows::set_activation_policy(0);
                            win.open_devtools();
                        } else {
                            println!("[devtools] No window found");
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

            // Sync autostart state from config on startup
            let cfg = config::load_config();
            let autostart = app.autolaunch();
            if cfg.autostart_enabled {
                let _ = autostart.enable();
            } else {
                let _ = autostart.disable();
            }

            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.hide();
            }

            // Check screen recording permission on startup (macOS only)
            // CGRequestScreenCaptureAccess() shows system dialog if user hasn't decided yet,
            // but always returns false immediately (before user responds).
            // We only show our custom window if preflight() returns false AND
            // we know the user has already seen the system dialog (i.e., second+ launch).
            #[cfg(target_os = "macos")]
            {
                if !permission::has_screen_recording_permission() {
                    // This triggers system dialog on first run (non-blocking, returns false immediately)
                    // On subsequent runs, returns false without dialog if denied
                    let _ = permission::request_screen_recording_permission();
                    // Don't show our custom window here - system dialog handles it
                    // User will see our window when they try to capture if still no permission
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
