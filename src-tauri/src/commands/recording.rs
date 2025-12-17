use std::thread;
use std::time::{Duration, Instant};

use crate::capture::Screen;
use image::RgbaImage;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::SharedState;
use crate::tray::{create_recording_overlay, update_tray_icon};
use crate::types::{RecordingInfo, RecordingState};
use crate::windows::{open_editor_window, set_activation_policy};

#[tauri::command]
pub fn start_recording(app: AppHandle, state: tauri::State<SharedState>) -> Result<(), String> {
    println!("[DEBUG][start_recording] ====== 被调用 ======");
    let mut s = state.lock().unwrap();
    if s.recording {
        println!("[DEBUG][start_recording] 已经在录制中，跳过");
        return Err("Already recording".to_string());
    }

    let region = s.region.clone().ok_or("No region selected")?;
    println!(
        "[DEBUG][start_recording] region: x={}, y={}, w={}, h={}",
        region.x, region.y, region.width, region.height
    );
    s.recording = true;
    s.frames.clear();

    let recording_fps = s.recording_fps;
    drop(s);

    update_tray_icon(&app, true);
    create_recording_overlay(&app, &region, false);

    let state_clone = state.inner().clone();
    let app_clone = app.clone();

    thread::spawn(move || {
        println!("[DEBUG][recording_thread] 录制线程启动");
        let screens = Screen::all().unwrap_or_default();
        if screens.is_empty() {
            println!("[DEBUG][recording_thread] 错误: 没有找到屏幕");
            return;
        }
        let screen = &screens[0];
        println!(
            "[DEBUG][recording_thread] 屏幕: {}x{}, scale={}, fps={}",
            screen.display_info.width,
            screen.display_info.height,
            screen.display_info.scale_factor,
            recording_fps
        );
        let frame_duration = Duration::from_millis(1000 / recording_fps as u64);

        let mut frame_idx = 0u32;
        loop {
            let start = Instant::now();

            {
                let s = state_clone.lock().unwrap();
                if !s.recording {
                    let frame_count = s.frames.len();
                    println!(
                        "[DEBUG][recording_thread] 录制停止，共捕获 {} 帧",
                        frame_count
                    );
                    drop(s);

                    update_tray_icon(&app_clone, false);

                    if let Some(overlay) = app_clone.get_webview_window("recording-overlay") {
                        let _ = overlay.close();
                    }

                    // Switch to Regular activation policy so window stays visible after cmd+tab
                    set_activation_policy(0);

                    // Open a new editor window
                    if let Err(e) = open_editor_window(&app_clone) {
                        println!("[DEBUG][recording_thread] 打开编辑器窗口失败: {}", e);
                    }

                    let _ = app_clone.emit(
                        "recording-stopped",
                        serde_json::json!({
                            "frame_count": frame_count
                        }),
                    );
                    break;
                }
            }

            match screen.capture_area(region.x, region.y, region.width, region.height) {
                Ok(img) => {
                    let rgba =
                        RgbaImage::from_raw(img.width(), img.height(), img.into_raw()).unwrap();

                    let mut s = state_clone.lock().unwrap();
                    s.frames.push(rgba);
                    frame_idx += 1;

                    if frame_idx <= 3 || frame_idx % 10 == 0 {
                        println!("[DEBUG][recording_thread] 捕获帧 #{}", frame_idx);
                    }

                    let _ = app_clone.emit(
                        "recording-state",
                        RecordingState {
                            is_recording: true,
                            frame_count: s.frames.len() as u32,
                        },
                    );
                }
                Err(e) => {
                    if frame_idx == 0 {
                        println!("[DEBUG][recording_thread] capture_area 失败: {:?}", e);
                        println!(
                            "[DEBUG][recording_thread] 参数: x={}, y={}, w={}, h={}",
                            region.x, region.y, region.width, region.height
                        );
                    }
                }
            }

            let elapsed = start.elapsed();
            if elapsed < frame_duration {
                thread::sleep(frame_duration - elapsed);
            }
        }
        println!("[DEBUG][recording_thread] 线程退出");
    });

    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: tauri::State<SharedState>) {
    println!("[DEBUG][stop_recording] ====== 被调用 ======");
    let mut s = state.lock().unwrap();
    s.recording = false;
    println!("[DEBUG][stop_recording] 录制标志已设置为 false");
}

#[tauri::command]
pub fn get_recording_info(state: tauri::State<SharedState>) -> RecordingInfo {
    let s = state.lock().unwrap();
    let (width, height) = if let Some(frame) = s.frames.first() {
        frame.dimensions()
    } else {
        (0, 0)
    };
    let duration_ms = if s.recording_fps > 0 {
        (s.frames.len() as u64 * 1000) / s.recording_fps as u64
    } else {
        0
    };

    RecordingInfo {
        frame_count: s.frames.len(),
        width,
        height,
        fps: s.recording_fps,
        duration_ms,
        has_frames: !s.frames.is_empty(),
    }
}

#[tauri::command]
pub fn discard_recording(app: AppHandle, state: tauri::State<SharedState>) {
    println!("[DEBUG][discard_recording] 丢弃录制数据");
    let mut s = state.lock().unwrap();
    s.frames.clear();
    drop(s);

    // Hide main window and switch back to Accessory policy
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.hide();
        set_activation_policy(1);
    }
}
