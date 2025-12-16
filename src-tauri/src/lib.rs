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
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindowBuilder, WebviewUrl, WindowEvent};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::image::Image as TauriImage;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tauri_plugin_clipboard_manager::ClipboardExt;

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

#[derive(Clone, Serialize, Deserialize)]
pub struct SaveResult {
    pub success: bool,
    pub path: Option<String>,
    pub error: Option<String>,
}

// 导出配置（用于 GIF 编辑器）
#[derive(Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    pub start_frame: usize,
    pub end_frame: usize,
    pub output_scale: f32,
    pub target_fps: u32,
    pub loop_mode: String, // "infinite", "once", "pingpong"
}

// 录制信息（供前端编辑器使用）
#[derive(Clone, Serialize, Deserialize)]
pub struct RecordingInfo {
    pub frame_count: usize,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration_ms: u64,
    pub has_frames: bool,
}

// 体积预估结果
#[derive(Clone, Serialize, Deserialize)]
pub struct SizeEstimate {
    pub frame_count: usize,
    pub output_width: u32,
    pub output_height: u32,
    pub estimated_bytes: u64,
    pub formatted: String,
}

// 导出进度
#[derive(Clone, Serialize, Deserialize)]
pub struct ExportProgress {
    pub current: usize,
    pub total: usize,
    pub stage: String, // "encoding", "scaling", etc.
}

#[derive(Clone, Default)]
enum GifLoopMode {
    #[default]
    Infinite,
    Once,
    PingPong,
}

struct AppState {
    recording: bool,
    region: Option<Region>,
    frames: Vec<RgbaImage>,
    recording_fps: u32,  // 录制时的帧率（固定30fps保证质量）
    // Screen info for DPI handling
    screen_x: i32,
    screen_y: i32,
    screen_scale: f32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            recording: false,
            region: None,
            frames: Vec::new(),
            recording_fps: 30,  // 固定30fps录制
            screen_x: 0,
            screen_y: 0,
            screen_scale: 1.0,
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
fn open_selector(app: AppHandle, state: tauri::State<SharedState>) -> Result<(), String> {
    println!("[DEBUG][open_selector] 入口");

    // If selector already exists, don't recreate (prevents rapid re-trigger)
    if let Some(win) = app.get_webview_window("selector") {
        println!("[DEBUG][open_selector] selector 窗口已存在，跳过");
        // Just ensure it's visible and focused
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    // Hide main window
    if let Some(main_win) = app.get_webview_window("main") {
        println!("[DEBUG][open_selector] 隐藏主窗口");
        let _ = main_win.hide();
    }

    // Use screenshots crate for full screen size (including dock/menu bar)
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

    // Store screen info for capture
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
        .accept_first_mouse(true)
        .build()
        .map_err(|e| e.to_string())?;

    // screenshots crate returns logical pixels, convert to physical
    let physical_width = (width as f32 * scale) as u32;
    let physical_height = (height as f32 * scale) as u32;
    let physical_x = (screen_x as f32 * scale) as i32;
    let physical_y = (screen_y as f32 * scale) as i32;

    win.set_size(PhysicalSize::new(physical_width, physical_height)).map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(physical_x, physical_y)).map_err(|e| e.to_string())?;

    // Set window level above dock on macOS
    #[cfg(target_os = "macos")]
    {
        use objc::{msg_send, sel, sel_impl};

        let _ = win.with_webview(|webview| {
            unsafe {
                let ns_window = webview.ns_window() as *mut objc::runtime::Object;
                // NSScreenSaverWindowLevel = 1000, above dock (20)
                let _: () = msg_send![ns_window, setLevel: 1000_i64];
            }
        });
    }

    Ok(())
}

#[tauri::command]
fn set_region(state: tauri::State<SharedState>, region: Region) {
    println!("[DEBUG][set_region] ====== 被调用 ====== x={}, y={}, w={}, h={}", region.x, region.y, region.width, region.height);
    let mut s = state.lock().unwrap();
    // screenshots crate 的 capture_area 使用逻辑像素(points)坐标，不需要转换
    // 浏览器的 clientX/clientY 已经是正确的逻辑像素
    println!("[DEBUG][set_region] 直接使用逻辑像素坐标（不缩放）");
    s.region = Some(region);
}

#[tauri::command]
fn start_recording(app: AppHandle, state: tauri::State<SharedState>) -> Result<(), String> {
    println!("[DEBUG][start_recording] ====== 被调用 ======");
    let mut s = state.lock().unwrap();
    if s.recording {
        println!("[DEBUG][start_recording] 已经在录制中，跳过");
        return Err("Already recording".to_string());
    }

    let region = s.region.clone().ok_or("No region selected")?;
    println!("[DEBUG][start_recording] region: x={}, y={}, w={}, h={}", region.x, region.y, region.width, region.height);
    s.recording = true;
    s.frames.clear();

    let recording_fps = s.recording_fps;  // 使用固定录制帧率
    drop(s);

    // 录制时不显示窗口，只更新托盘图标
    update_tray_icon(&app, true);

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
        println!("[DEBUG][recording_thread] 屏幕: {}x{}, scale={}, fps={}",
            screen.display_info.width, screen.display_info.height, screen.display_info.scale_factor, recording_fps);
        let frame_duration = Duration::from_millis(1000 / recording_fps as u64);

        let mut frame_idx = 0u32;
        loop {
            let start = Instant::now();

            {
                let s = state_clone.lock().unwrap();
                if !s.recording {
                    let frame_count = s.frames.len();
                    println!("[DEBUG][recording_thread] 录制停止，共捕获 {} 帧", frame_count);
                    drop(s);

                    // 恢复托盘图标
                    update_tray_icon(&app_clone, false);

                    // 显示主窗口进入编辑模式
                    if let Some(main_win) = app_clone.get_webview_window("main") {
                        let _ = main_win.show();
                        let _ = main_win.set_focus();
                    }

                    // 录制线程退出时发送事件，确保所有帧已写入
                    let _ = app_clone.emit("recording-stopped", serde_json::json!({
                        "frame_count": frame_count
                    }));
                    break;
                }
            }

            // Capture screen region
            match screen.capture_area(region.x, region.y, region.width, region.height) {
                Ok(img) => {
                    let rgba = RgbaImage::from_raw(
                        img.width(),
                        img.height(),
                        img.into_raw(),
                    ).unwrap();

                    let mut s = state_clone.lock().unwrap();
                    s.frames.push(rgba);
                    frame_idx += 1;

                    if frame_idx <= 3 || frame_idx % 10 == 0 {
                        println!("[DEBUG][recording_thread] 捕获帧 #{}", frame_idx);
                    }

                    let _ = app_clone.emit("recording-state", RecordingState {
                        is_recording: true,
                        frame_count: s.frames.len() as u32,
                    });
                }
                Err(e) => {
                    if frame_idx == 0 {
                        println!("[DEBUG][recording_thread] capture_area 失败: {:?}", e);
                        println!("[DEBUG][recording_thread] 参数: x={}, y={}, w={}, h={}",
                            region.x, region.y, region.width, region.height);
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
fn stop_recording(state: tauri::State<SharedState>) {
    println!("[DEBUG][stop_recording] ====== 被调用 ======");
    let mut s = state.lock().unwrap();
    s.recording = false;
    println!("[DEBUG][stop_recording] 录制标志已设置为 false");
    // 事件由录制线程退出时发送，避免竞态条件
}

#[tauri::command]
fn get_recording_info(state: tauri::State<SharedState>) -> RecordingInfo {
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
fn estimate_export_size(state: tauri::State<SharedState>, config: ExportConfig) -> SizeEstimate {
    let s = state.lock().unwrap();

    let (orig_width, orig_height) = if let Some(frame) = s.frames.first() {
        frame.dimensions()
    } else {
        return SizeEstimate {
            frame_count: 0,
            output_width: 0,
            output_height: 0,
            estimated_bytes: 0,
            formatted: "0 B".to_string(),
        };
    };

    // 计算裁剪后的帧数
    let start = config.start_frame.min(s.frames.len());
    let end = config.end_frame.min(s.frames.len());
    let trimmed_count = if end > start { end - start } else { 0 };

    // 计算降帧后的帧数（通过跳帧实现）
    let frame_step = if config.target_fps > 0 && config.target_fps < s.recording_fps {
        s.recording_fps / config.target_fps
    } else {
        1
    };
    let final_frame_count = (trimmed_count + frame_step as usize - 1) / frame_step as usize;

    // 计算输出尺寸
    let output_width = (orig_width as f32 * config.output_scale) as u32;
    let output_height = (orig_height as f32 * config.output_scale) as u32;

    // PingPong 模式会增加帧数
    let total_frames = if config.loop_mode == "pingpong" && final_frame_count > 2 {
        final_frame_count * 2 - 2
    } else {
        final_frame_count
    };

    // 经验估算：GIF 每像素约 0.12-0.18 字节（考虑 LZW 压缩）
    let bytes_per_pixel = 0.15;
    let estimated_bytes = (total_frames as f64 * output_width as f64 * output_height as f64 * bytes_per_pixel) as u64;

    // 格式化文件大小
    let formatted = format_bytes(estimated_bytes);

    SizeEstimate {
        frame_count: total_frames,
        output_width,
        output_height,
        estimated_bytes,
        formatted,
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[tauri::command]
fn discard_recording(state: tauri::State<SharedState>) {
    println!("[DEBUG][discard_recording] 丢弃录制数据");
    let mut s = state.lock().unwrap();
    s.frames.clear();
}

#[tauri::command]
fn get_frame_thumbnail(state: tauri::State<SharedState>, frame_index: usize, max_height: u32) -> Result<String, String> {
    let s = state.lock().unwrap();

    if frame_index >= s.frames.len() {
        return Err("Frame index out of bounds".to_string());
    }

    let frame = &s.frames[frame_index];
    let (orig_w, orig_h) = frame.dimensions();

    // 按高度等比缩放
    let scale = max_height as f32 / orig_h as f32;
    let thumb_w = (orig_w as f32 * scale) as u32;
    let thumb_h = max_height;

    let thumbnail = image::imageops::resize(frame, thumb_w, thumb_h, image::imageops::FilterType::Triangle);

    // 编码为 PNG base64
    use image::ImageEncoder;
    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder.write_image(
        thumbnail.as_raw(),
        thumb_w,
        thumb_h,
        image::ExtendedColorType::Rgba8,
    ).map_err(|e| e.to_string())?;

    let base64_str = STANDARD.encode(&png_data);
    Ok(format!("data:image/png;base64,{}", base64_str))
}

/// 获取 filmstrip 缩略图条（均匀采样 count 帧）
#[tauri::command]
fn get_filmstrip(state: tauri::State<SharedState>, count: usize, thumb_height: u32) -> Result<Vec<String>, String> {
    let s = state.lock().unwrap();
    let total = s.frames.len();

    if total == 0 {
        return Err("No frames available".to_string());
    }

    let count = count.min(total).max(1);
    let step = if count > 1 { (total - 1) as f32 / (count - 1) as f32 } else { 0.0 };

    let mut thumbnails = Vec::with_capacity(count);

    for i in 0..count {
        let frame_idx = if count > 1 {
            ((i as f32 * step).round() as usize).min(total - 1)
        } else {
            0
        };

        let frame = &s.frames[frame_idx];
        let (orig_w, orig_h) = frame.dimensions();

        // 按高度等比缩放
        let scale = thumb_height as f32 / orig_h as f32;
        let thumb_w = (orig_w as f32 * scale) as u32;

        let thumbnail = image::imageops::resize(frame, thumb_w, thumb_height, image::imageops::FilterType::Nearest);

        // 转为 RGB 后编码为 JPEG（更小）
        let rgb_thumbnail = image::DynamicImage::ImageRgba8(thumbnail).to_rgb8();
        let mut jpg_data = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut jpg_data);
        rgb_thumbnail.write_to(&mut cursor, image::ImageFormat::Jpeg).map_err(|e| e.to_string())?;

        let base64_str = STANDARD.encode(&jpg_data);
        thumbnails.push(format!("data:image/jpeg;base64,{}", base64_str));
    }

    Ok(thumbnails)
}

#[tauri::command]
fn save_screenshot(app: AppHandle, state: tauri::State<SharedState>, scale: Option<f32>) -> Result<String, String> {
    println!("[DEBUG][save_screenshot] ====== 被调用 ======");
    let s = state.lock().unwrap();
    let region = s.region.clone().ok_or("No region selected")?;
    let output_scale = scale.unwrap_or(1.0).clamp(0.1, 1.0);
    println!("[DEBUG][save_screenshot] region: x={}, y={}, w={}, h={}, scale={}",
        region.x, region.y, region.width, region.height, output_scale);
    drop(s);

    let screens = Screen::all().map_err(|e| {
        println!("[DEBUG][save_screenshot] Screen::all 错误: {}", e);
        e.to_string()
    })?;
    if screens.is_empty() {
        println!("[DEBUG][save_screenshot] 没有找到屏幕");
        return Err("No screens found".to_string());
    }
    println!("[DEBUG][save_screenshot] 找到 {} 个屏幕", screens.len());

    let screen = &screens[0];
    println!("[DEBUG][save_screenshot] 调用 capture_area: x={}, y={}, w={}, h={}", region.x, region.y, region.width, region.height);
    let captured = screen.capture_area(region.x, region.y, region.width, region.height)
        .map_err(|e| {
            println!("[DEBUG][save_screenshot] capture_area 错误: {}", e);
            e.to_string()
        })?;
    println!("[DEBUG][save_screenshot] capture_area 成功, 图像尺寸: {}x{}", captured.width(), captured.height());

    // Convert from screenshots' image type to our image type
    let captured_rgba = RgbaImage::from_raw(
        captured.width(),
        captured.height(),
        captured.into_raw(),
    ).ok_or("Failed to convert image")?;

    // Apply scaling if needed
    let img = if (output_scale - 1.0).abs() > 0.01 {
        let new_w = (captured_rgba.width() as f32 * output_scale) as u32;
        let new_h = (captured_rgba.height() as f32 * output_scale) as u32;
        println!("[DEBUG][save_screenshot] 缩放到: {}x{}", new_w, new_h);
        image::imageops::resize(&captured_rgba, new_w, new_h, image::imageops::FilterType::Lanczos3)
    } else {
        captured_rgba
    };

    // 复制到剪切板 (使用 RGBA raw bytes)
    let tauri_image = tauri::image::Image::new_owned(
        img.as_raw().to_vec(),
        img.width(),
        img.height(),
    );
    app.clipboard().write_image(&tauri_image).map_err(|e| {
        println!("[DEBUG][save_screenshot] 复制到剪切板错误: {}", e);
        e.to_string()
    })?;
    println!("[DEBUG][save_screenshot] 已复制到剪切板");

    // 保存文件
    let output_dir = dirs::picture_dir()
        .or_else(|| dirs::home_dir())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lovshot");
    println!("[DEBUG][save_screenshot] 输出目录: {:?}", output_dir);

    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = output_dir.join(format!("screenshot_{}.png", timestamp));
    println!("[DEBUG][save_screenshot] 保存文件: {:?}", filename);

    img.save(&filename).map_err(|e| {
        println!("[DEBUG][save_screenshot] 保存文件错误: {}", e);
        e.to_string()
    })?;
    println!("[DEBUG][save_screenshot] 文件保存成功");

    Ok(filename.to_string_lossy().to_string())
}

#[tauri::command]
fn export_gif(app: AppHandle, state: tauri::State<SharedState>, config: ExportConfig) -> Result<(), String> {
    println!("[DEBUG][export_gif] ====== 被调用 ======");
    println!("[DEBUG][export_gif] config: start={}, end={}, scale={}, fps={}, loop={}",
        config.start_frame, config.end_frame, config.output_scale, config.target_fps, config.loop_mode);

    let mut s = state.lock().unwrap();

    if s.frames.is_empty() {
        println!("[DEBUG][export_gif] 错误: 没有帧可保存");
        let _ = app.emit("export-complete", SaveResult {
            success: false,
            path: None,
            error: Some("No frames to export".to_string()),
        });
        return Ok(());
    }

    let total_frames = s.frames.len();
    let recording_fps = s.recording_fps;
    println!("[DEBUG][export_gif] 原始帧数: {}, 录制帧率: {}", total_frames, recording_fps);

    // 取出帧数据
    let all_frames = std::mem::take(&mut s.frames);
    drop(s);

    // 克隆配置用于线程
    let config = config.clone();

    // 在后台线程编码，立即返回
    thread::spawn(move || {
        // 1. 裁剪帧范围
        let start = config.start_frame.min(total_frames);
        let end = config.end_frame.min(total_frames);
        if end <= start {
            let _ = app.emit("export-complete", SaveResult {
                success: false,
                path: None,
                error: Some("Invalid frame range".to_string()),
            });
            return;
        }
        let trimmed_frames: Vec<_> = all_frames[start..end].to_vec();
        println!("[DEBUG][export_gif] 裁剪后帧数: {}", trimmed_frames.len());

        // 2. 降帧（通过跳帧实现）
        let frame_step = if config.target_fps > 0 && config.target_fps < recording_fps {
            (recording_fps / config.target_fps) as usize
        } else {
            1
        };
        let sampled_frames: Vec<_> = trimmed_frames.into_iter()
            .step_by(frame_step)
            .collect();
        println!("[DEBUG][export_gif] 降帧后: step={}, 帧数={}", frame_step, sampled_frames.len());

        if sampled_frames.is_empty() {
            let _ = app.emit("export-complete", SaveResult {
                success: false,
                path: None,
                error: Some("No frames after sampling".to_string()),
            });
            return;
        }

        // 3. 缩放
        let output_scale = config.output_scale.clamp(0.1, 1.0);
        let scaled_frames: Vec<RgbaImage> = if (output_scale - 1.0).abs() > 0.01 {
            println!("[DEBUG][export_gif] 缩放帧: scale={}", output_scale);
            sampled_frames.into_iter().map(|f| {
                let new_w = (f.width() as f32 * output_scale) as u32;
                let new_h = (f.height() as f32 * output_scale) as u32;
                image::imageops::resize(&f, new_w, new_h, image::imageops::FilterType::Triangle)
            }).collect()
        } else {
            sampled_frames
        };

        // 4. 处理 PingPong 模式
        let gif_loop_mode = match config.loop_mode.as_str() {
            "once" => GifLoopMode::Once,
            "pingpong" => GifLoopMode::PingPong,
            _ => GifLoopMode::Infinite,
        };

        let final_frames: Vec<RgbaImage> = match gif_loop_mode {
            GifLoopMode::PingPong if scaled_frames.len() > 2 => {
                let mut result = scaled_frames.clone();
                let reversed: Vec<_> = scaled_frames[1..scaled_frames.len()-1].iter().rev().cloned().collect();
                result.extend(reversed);
                println!("[DEBUG][export_gif] PingPong 模式: {} -> {} 帧", scaled_frames.len(), result.len());
                result
            }
            _ => scaled_frames,
        };

        // 5. 编码 GIF
        let output_dir = dirs::picture_dir()
            .or_else(|| dirs::home_dir())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lovshot");

        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            let _ = app.emit("export-complete", SaveResult {
                success: false,
                path: None,
                error: Some(e.to_string()),
            });
            return;
        }

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = output_dir.join(format!("recording_{}.gif", timestamp));
        println!("[DEBUG][export_gif] 保存路径: {:?}", filename);

        let (width, height) = final_frames[0].dimensions();
        let frame_count = final_frames.len();
        println!("[DEBUG][export_gif] 开始编码: {}x{}, {} 帧", width, height, frame_count);

        let result = (|| -> Result<String, String> {
            let mut file = File::create(&filename).map_err(|e| e.to_string())?;
            let mut encoder = Encoder::new(&mut file, width as u16, height as u16, &[])
                .map_err(|e| e.to_string())?;

            // 设置循环模式
            let repeat = match gif_loop_mode {
                GifLoopMode::Once => Repeat::Finite(0),
                _ => Repeat::Infinite,
            };
            encoder.set_repeat(repeat).map_err(|e| e.to_string())?;

            // GIF delay 单位是 1/100 秒
            let delay = if config.target_fps > 0 {
                (100 / config.target_fps) as u16
            } else {
                10 // 默认 10fps
            };

            for (i, rgba_img) in final_frames.into_iter().enumerate() {
                let mut pixels: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
                for pixel in rgba_img.pixels() {
                    pixels.push(pixel[0]);
                    pixels.push(pixel[1]);
                    pixels.push(pixel[2]);
                    pixels.push(pixel[3]);
                }

                let mut frame = Frame::from_rgba_speed(width as u16, height as u16, &mut pixels, 30);
                frame.delay = delay;
                encoder.write_frame(&frame).map_err(|e| e.to_string())?;

                // 发送进度事件
                let _ = app.emit("export-progress", ExportProgress {
                    current: i + 1,
                    total: frame_count,
                    stage: "encoding".to_string(),
                });

                if i == 0 || (i + 1) % 10 == 0 || i + 1 == frame_count {
                    println!("[DEBUG][export_gif] 编码帧 {}/{}", i + 1, frame_count);
                }
            }

            Ok(filename.to_string_lossy().to_string())
        })();

        match result {
            Ok(path) => {
                println!("[DEBUG][export_gif] ====== 完成 ====== 路径: {}", path);
                let _ = app.emit("export-complete", SaveResult {
                    success: true,
                    path: Some(path),
                    error: None,
                });
            }
            Err(e) => {
                println!("[DEBUG][export_gif] ====== 错误 ====== {}", e);
                let _ = app.emit("export-complete", SaveResult {
                    success: false,
                    path: None,
                    error: Some(e),
                });
            }
        }
    });

    Ok(())
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
                .with_handler(move |app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        println!("[DEBUG][shortcut] Option+A pressed");
                        let is_recording = state_for_shortcut.lock().unwrap().recording;
                        if is_recording {
                            // 正在录制时，按快捷键停止
                            println!("[DEBUG][shortcut] 停止录制");
                            state_for_shortcut.lock().unwrap().recording = false;
                        } else {
                            // 未录制时，打开选择器
                            let _ = open_selector_internal(app.clone());
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_screens,
            capture_screenshot,
            open_selector,
            set_region,
            start_recording,
            stop_recording,
            get_recording_info,
            estimate_export_size,
            export_gif,
            discard_recording,
            get_frame_thumbnail,
            get_filmstrip,
            save_screenshot,
        ])
        .on_window_event(|window, event| {
            // macOS: Cmd+W 或点击关闭按钮时隐藏窗口而非退出
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    window.hide().unwrap();
                    api.prevent_close();
                }
            }
        })
        .setup(move |app| {
            // 创建系统托盘
            let tray_icon = load_tray_icon(false)
                .unwrap_or_else(|| app.default_window_icon().unwrap().clone());

            let state_clone = state_for_tray.clone();
            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("Lovshot - Option+A to capture")
                .on_tray_icon_event(move |tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        let is_recording = state_clone.lock().unwrap().recording;
                        if is_recording {
                            // 点击托盘停止录制
                            println!("[DEBUG][tray] 点击托盘停止录制");
                            state_clone.lock().unwrap().recording = false;
                        } else {
                            // 点击托盘打开选择器
                            let _ = open_selector_internal(app.clone());
                        }
                    }
                })
                .build(app)?;

            // 注册全局快捷键 Option+A
            let shortcut = Shortcut::new(Some(Modifiers::ALT), Code::KeyA);
            app.global_shortcut().register(shortcut)?;
            println!("[DEBUG] Global shortcut Option+A registered");

            // 隐藏主窗口（仅在编辑时显示）
            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.hide();
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 加载托盘图标
fn load_tray_icon(is_recording: bool) -> Option<TauriImage<'static>> {
    let icon_bytes: &[u8] = if is_recording {
        include_bytes!("../icons/tray-recording.png")
    } else {
        include_bytes!("../icons/tray-icon.png")
    };

    // 解析 PNG 获取尺寸和 RGBA 数据
    let img = image::load_from_memory(icon_bytes).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(TauriImage::new_owned(rgba.into_raw(), width, height))
}

/// 更新托盘图标（录制状态）
fn update_tray_icon(app: &AppHandle, is_recording: bool) {
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

// Internal function to open selector (called from shortcut handler)
fn open_selector_internal(app: AppHandle) -> Result<(), String> {
    println!("[DEBUG][open_selector_internal] 入口");

    // If selector already exists, don't recreate
    if let Some(win) = app.get_webview_window("selector") {
        println!("[DEBUG][open_selector_internal] selector 窗口已存在，跳过");
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    // Hide main window
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.hide();
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

    // Store screen info
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
        .accept_first_mouse(true)
        .build()
        .map_err(|e| e.to_string())?;

    let physical_width = (width as f32 * scale) as u32;
    let physical_height = (height as f32 * scale) as u32;
    let physical_x = (screen_x as f32 * scale) as i32;
    let physical_y = (screen_y as f32 * scale) as i32;

    win.set_size(PhysicalSize::new(physical_width, physical_height)).map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(physical_x, physical_y)).map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    {
        use objc::{msg_send, sel, sel_impl};
        let _ = win.with_webview(|webview| {
            unsafe {
                let ns_window = webview.ns_window() as *mut objc::runtime::Object;
                let _: () = msg_send![ns_window, setLevel: 1000_i64];
            }
        });
    }

    Ok(())
}
