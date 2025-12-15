use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::fs::File;
use std::path::PathBuf;

use base64::{Engine, engine::general_purpose::STANDARD};
use gif::{Encoder, Frame, Repeat};
use image::RgbaImage;
use screenshots::Screen;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindowBuilder, WebviewUrl};

#[derive(Clone, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RecordingState {
    pub is_recording: bool,
    pub frame_count: u32,
}

struct AppState {
    recording: bool,
    region: Option<Region>,
    frames: Vec<RgbaImage>,
    fps: u32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            recording: false,
            region: None,
            frames: Vec::new(),
            fps: 10,
        }
    }
}

type SharedState = Arc<Mutex<AppState>>;

#[tauri::command]
fn get_screens() -> Vec<serde_json::Value> {
    Screen::all()
        .unwrap_or_default()
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.display_info.id,
                "x": s.display_info.x,
                "y": s.display_info.y,
                "width": s.display_info.width,
                "height": s.display_info.height,
                "scale": s.display_info.scale_factor,
            })
        })
        .collect()
}

#[tauri::command]
fn capture_screenshot() -> Result<String, String> {
    let screens = Screen::all().map_err(|e| e.to_string())?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let screen = &screens[0];
    let img = screen.capture().map_err(|e| e.to_string())?;

    // Convert to base64 PNG
    use image::ImageEncoder;
    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder.write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    ).map_err(|e| e.to_string())?;

    let base64_str = STANDARD.encode(&png_data);
    Ok(format!("data:image/png;base64,{}", base64_str))
}

#[tauri::command]
fn open_selector(app: AppHandle) -> Result<(), String> {
    let screens = Screen::all().map_err(|e| e.to_string())?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let screen = &screens[0];
    let width = screen.display_info.width;
    let height = screen.display_info.height;
    let scale = screen.display_info.scale_factor;

    // Close existing selector if any
    if let Some(win) = app.get_webview_window("selector") {
        let _ = win.close();
    }

    // Create fullscreen overlay for selection (use logical size)
    let logical_width = width as f64 / scale as f64;
    let logical_height = height as f64 / scale as f64;

    WebviewWindowBuilder::new(&app, "selector", WebviewUrl::App("/selector.html".into()))
        .title("Select Region")
        .inner_size(logical_width, logical_height)
        .position(0.0, 0.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn set_region(state: tauri::State<SharedState>, region: Region) {
    let mut s = state.lock().unwrap();
    s.region = Some(region);
}

#[tauri::command]
fn start_recording(app: AppHandle, state: tauri::State<SharedState>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    if s.recording {
        return Err("Already recording".to_string());
    }

    let region = s.region.clone().ok_or("No region selected")?;
    s.recording = true;
    s.frames.clear();

    let fps = s.fps;
    drop(s);

    let state_clone = state.inner().clone();
    let app_clone = app.clone();

    thread::spawn(move || {
        let screens = Screen::all().unwrap_or_default();
        if screens.is_empty() {
            return;
        }
        let screen = &screens[0];
        let frame_duration = Duration::from_millis(1000 / fps as u64);

        loop {
            let start = Instant::now();

            {
                let s = state_clone.lock().unwrap();
                if !s.recording {
                    break;
                }
            }

            // Capture screen region
            if let Ok(img) = screen.capture_area(
                region.x,
                region.y,
                region.width,
                region.height,
            ) {
                let rgba = RgbaImage::from_raw(
                    img.width(),
                    img.height(),
                    img.into_raw(),
                ).unwrap();

                let mut s = state_clone.lock().unwrap();
                s.frames.push(rgba);

                let _ = app_clone.emit("recording-state", RecordingState {
                    is_recording: true,
                    frame_count: s.frames.len() as u32,
                });
            }

            let elapsed = start.elapsed();
            if elapsed < frame_duration {
                thread::sleep(frame_duration - elapsed);
            }
        }
    });

    Ok(())
}

#[tauri::command]
fn stop_recording(state: tauri::State<SharedState>) {
    let mut s = state.lock().unwrap();
    s.recording = false;
}

#[tauri::command]
fn save_screenshot(state: tauri::State<SharedState>) -> Result<String, String> {
    let s = state.lock().unwrap();
    let region = s.region.clone().ok_or("No region selected")?;
    drop(s);

    let screens = Screen::all().map_err(|e| e.to_string())?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let screen = &screens[0];
    let img = screen.capture_area(region.x, region.y, region.width, region.height)
        .map_err(|e| e.to_string())?;

    let output_dir = dirs::picture_dir()
        .or_else(|| dirs::home_dir())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lovshot");

    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = output_dir.join(format!("screenshot_{}.png", timestamp));

    img.save(&filename).map_err(|e| e.to_string())?;

    Ok(filename.to_string_lossy().to_string())
}

#[tauri::command]
fn save_gif(state: tauri::State<SharedState>) -> Result<String, String> {
    let mut s = state.lock().unwrap();

    if s.frames.is_empty() {
        return Err("No frames to save".to_string());
    }

    // Get output directory
    let output_dir = dirs::picture_dir()
        .or_else(|| dirs::home_dir())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lovshot");

    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = output_dir.join(format!("recording_{}.gif", timestamp));

    let frames = std::mem::take(&mut s.frames);
    let fps = s.fps;
    drop(s);

    if frames.is_empty() {
        return Err("No frames captured".to_string());
    }

    let (width, height) = frames[0].dimensions();

    let mut file = File::create(&filename).map_err(|e| e.to_string())?;
    let mut encoder = Encoder::new(&mut file, width as u16, height as u16, &[])
        .map_err(|e| e.to_string())?;

    encoder.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;

    let delay = (100 / fps) as u16; // centiseconds

    for rgba_img in frames {
        let mut pixels: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
        for pixel in rgba_img.pixels() {
            pixels.push(pixel[0]); // R
            pixels.push(pixel[1]); // G
            pixels.push(pixel[2]); // B
            pixels.push(pixel[3]); // A
        }

        let mut frame = Frame::from_rgba_speed(width as u16, height as u16, &mut pixels, 10);
        frame.delay = delay;
        encoder.write_frame(&frame).map_err(|e| e.to_string())?;
    }

    Ok(filename.to_string_lossy().to_string())
}

#[tauri::command]
fn set_fps(state: tauri::State<SharedState>, fps: u32) {
    let mut s = state.lock().unwrap();
    s.fps = fps.clamp(1, 30);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state: SharedState = Arc::new(Mutex::new(AppState::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_screens,
            capture_screenshot,
            open_selector,
            set_region,
            start_recording,
            stop_recording,
            save_gif,
            set_fps,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
