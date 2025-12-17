use std::fs::File;
use std::path::PathBuf;
use std::thread;

use base64::{Engine, engine::general_purpose::STANDARD};
use gif::{Encoder, Frame, Repeat};
use image::RgbaImage;
use crate::capture::Screen;
use tauri::{AppHandle, Emitter};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::state::SharedState;
use crate::types::{ExportConfig, ExportProgress, GifLoopMode, SaveResult, SizeEstimate};

#[tauri::command]
pub fn estimate_export_size(state: tauri::State<SharedState>, config: ExportConfig) -> SizeEstimate {
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

    let start = config.start_frame.min(s.frames.len());
    let end = config.end_frame.min(s.frames.len());
    let trimmed_count = if end > start { end - start } else { 0 };

    // Output duration = original duration / speed
    // Output frames = output duration × target_fps
    // = (trimmed_count / recording_fps / speed) × target_fps
    let speed = config.speed.clamp(0.1, 10.0) as f64;
    let original_duration = trimmed_count as f64 / s.recording_fps as f64;
    let output_duration = original_duration / speed;
    let final_frame_count = (output_duration * config.target_fps as f64).round() as usize;

    let output_width = (orig_width as f32 * config.output_scale) as u32;
    let output_height = (orig_height as f32 * config.output_scale) as u32;

    let total_frames = if config.loop_mode == "pingpong" && final_frame_count > 2 {
        final_frame_count * 2 - 2
    } else {
        final_frame_count
    };

    // Adjust bytes_per_pixel based on quality (1-100)
    // Low quality (1) -> ~0.05, High quality (100) -> ~0.4 (8x difference)
    let quality_factor = config.quality.clamp(1, 100) as f64 / 100.0;
    let bytes_per_pixel = 0.05 + quality_factor * 0.35;
    let estimated_bytes = (total_frames as f64 * output_width as f64 * output_height as f64 * bytes_per_pixel) as u64;
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
pub fn get_frame_thumbnail(state: tauri::State<SharedState>, frame_index: usize, max_height: u32) -> Result<String, String> {
    let s = state.lock().unwrap();

    if frame_index >= s.frames.len() {
        return Err("Frame index out of bounds".to_string());
    }

    let frame = &s.frames[frame_index];
    let (orig_w, orig_h) = frame.dimensions();

    let scale = max_height as f32 / orig_h as f32;
    let thumb_w = (orig_w as f32 * scale) as u32;
    let thumb_h = max_height;

    let thumbnail = image::imageops::resize(frame, thumb_w, thumb_h, image::imageops::FilterType::Triangle);

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

#[tauri::command]
pub fn get_filmstrip(state: tauri::State<SharedState>, count: usize, thumb_height: u32) -> Result<Vec<String>, String> {
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

        let scale = thumb_height as f32 / orig_h as f32;
        let thumb_w = (orig_w as f32 * scale) as u32;

        let thumbnail = image::imageops::resize(frame, thumb_w, thumb_height, image::imageops::FilterType::Nearest);

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
pub fn save_screenshot(app: AppHandle, state: tauri::State<SharedState>, scale: Option<f32>) -> Result<String, String> {
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

    let captured_rgba = RgbaImage::from_raw(
        captured.width(),
        captured.height(),
        captured.into_raw(),
    ).ok_or("Failed to convert image")?;

    let img = if (output_scale - 1.0).abs() > 0.01 {
        let new_w = (captured_rgba.width() as f32 * output_scale) as u32;
        let new_h = (captured_rgba.height() as f32 * output_scale) as u32;
        println!("[DEBUG][save_screenshot] 缩放到: {}x{}", new_w, new_h);
        image::imageops::resize(&captured_rgba, new_w, new_h, image::imageops::FilterType::Lanczos3)
    } else {
        captured_rgba
    };

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
pub fn export_gif(app: AppHandle, state: tauri::State<SharedState>, config: ExportConfig) -> Result<(), String> {
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

    let all_frames = s.frames.clone();
    drop(s);

    let config = config.clone();

    thread::spawn(move || {
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
        let trimmed_count = trimmed_frames.len();
        println!("[DEBUG][export_gif] 裁剪后帧数: {}", trimmed_count);

        // Calculate target frame count based on output duration and fps
        // output_duration = original_duration / speed
        // output_frames = output_duration × target_fps
        let speed = config.speed.clamp(0.1, 10.0);
        let original_duration = trimmed_count as f32 / recording_fps as f32;
        let output_duration = original_duration / speed;
        let target_frame_count = (output_duration * config.target_fps as f32).round() as usize;
        let target_frame_count = target_frame_count.max(1);

        // Sample frames uniformly
        let sampled_frames: Vec<_> = if target_frame_count >= trimmed_count {
            trimmed_frames
        } else {
            (0..target_frame_count)
                .map(|i| {
                    let src_idx = (i as f32 * (trimmed_count - 1) as f32 / (target_frame_count - 1).max(1) as f32).round() as usize;
                    trimmed_frames[src_idx.min(trimmed_count - 1)].clone()
                })
                .collect()
        };
        println!("[DEBUG][export_gif] 采样后: target={}, 实际={}, speed={}", target_frame_count, sampled_frames.len(), speed);

        if sampled_frames.is_empty() {
            let _ = app.emit("export-complete", SaveResult {
                success: false,
                path: None,
                error: Some("No frames after sampling".to_string()),
            });
            return;
        }

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

        // Use custom path or default
        let filename = if let Some(ref custom_path) = config.output_path {
            PathBuf::from(custom_path)
        } else {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            output_dir.join(format!("recording_{}.gif", timestamp))
        };
        println!("[DEBUG][export_gif] 保存路径: {:?}", filename);

        let (width, height) = final_frames[0].dimensions();
        let frame_count = final_frames.len();
        println!("[DEBUG][export_gif] 开始编码: {}x{}, {} 帧", width, height, frame_count);

        let result = (|| -> Result<String, String> {
            let mut file = File::create(&filename).map_err(|e| e.to_string())?;
            let mut encoder = Encoder::new(&mut file, width as u16, height as u16, &[])
                .map_err(|e| e.to_string())?;

            let repeat = match gif_loop_mode {
                GifLoopMode::Once => Repeat::Finite(0),
                _ => Repeat::Infinite,
            };
            encoder.set_repeat(repeat).map_err(|e| e.to_string())?;

            // GIF delay is in 1/100 seconds: delay = 100 / fps
            // (speed already affects frame count, so delay is just based on fps)
            let delay = if config.target_fps > 0 {
                (100.0 / config.target_fps as f32).max(1.0) as u16
            } else {
                10
            };

            for (i, rgba_img) in final_frames.into_iter().enumerate() {
                let mut pixels: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
                for pixel in rgba_img.pixels() {
                    pixels.push(pixel[0]);
                    pixels.push(pixel[1]);
                    pixels.push(pixel[2]);
                    pixels.push(pixel[3]);
                }

                // Map quality (1-100) to gif speed (30-1): higher quality = lower speed = better but slower
                let gif_speed = 30 - ((config.quality.clamp(1, 100) - 1) * 29 / 99);
                let mut frame = Frame::from_rgba_speed(width as u16, height as u16, &mut pixels, gif_speed as i32);
                frame.delay = delay;
                encoder.write_frame(&frame).map_err(|e| e.to_string())?;

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

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn reveal_in_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .args(["/select,", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        // Try to open parent folder
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
pub struct HistoryItem {
    pub path: String,
    pub filename: String,
    pub file_type: String, // "screenshot" or "gif"
    pub modified: u64,     // unix timestamp
    pub size: u64,         // file size in bytes
    pub thumbnail: String, // base64 data URL
}

#[derive(serde::Serialize)]
pub struct HistoryResponse {
    pub items: Vec<HistoryItem>,
    pub has_more: bool,
    pub total: usize,
}

#[derive(serde::Serialize)]
pub struct StatsResponse {
    pub total_count: usize,
    pub screenshot_count: usize,
    pub gif_count: usize,
    pub total_size: u64,
    pub today_count: usize,
    pub week_count: usize,
}

struct FileInfo {
    path: PathBuf,
    filename: String,
    file_type: String,
    modified: u64,
    size: u64,
}

fn generate_thumbnail(path: &PathBuf, file_type: &str) -> String {
    // Max width 200px, preserve aspect ratio (no fixed height constraint)
    const MAX_WIDTH: u32 = 200;

    match file_type {
        "gif" => {
            if let Ok(file) = File::open(path) {
                if let Ok(mut decoder) = gif::DecodeOptions::new().read_info(file) {
                    if let Ok(Some(frame)) = decoder.read_next_frame() {
                        let w = frame.width as u32;
                        let h = frame.height as u32;
                        if let Some(img) = image::RgbaImage::from_raw(w, h, frame.buffer.to_vec()) {
                            // Scale to max width while preserving aspect ratio
                            let new_w = MAX_WIDTH.min(w);
                            let new_h = (h as f32 * new_w as f32 / w as f32) as u32;
                            let thumb = image::imageops::thumbnail(&img, new_w, new_h);
                            let mut buf = Vec::new();
                            if thumb.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).is_ok() {
                                return format!("data:image/png;base64,{}", STANDARD.encode(&buf));
                            }
                        }
                    }
                }
            }
            String::new()
        }
        _ => {
            if let Ok(img) = image::open(path) {
                let (w, h) = (img.width(), img.height());
                let new_w = MAX_WIDTH.min(w);
                let new_h = (h as f32 * new_w as f32 / w as f32) as u32;
                let thumb = img.thumbnail(new_w, new_h.max(1));
                let mut buf = Vec::new();
                if thumb.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).is_ok() {
                    return format!("data:image/png;base64,{}", STANDARD.encode(&buf));
                }
            }
            String::new()
        }
    }
}

#[tauri::command]
pub async fn get_history(
    offset: Option<usize>,
    limit: Option<usize>,
    filter_type: Option<String>,
) -> Result<HistoryResponse, String> {
    tokio::task::spawn_blocking(move || {
        let output_dir = dirs::picture_dir()
            .or_else(|| dirs::home_dir())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lovshot");

        if !output_dir.exists() {
            return Ok(HistoryResponse { items: vec![], has_more: false, total: 0 });
        }

        // Step 1: Collect file metadata only (no thumbnail generation)
        let entries = std::fs::read_dir(&output_dir).map_err(|e| e.to_string())?;
        let mut files: Vec<FileInfo> = vec![];

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

            let file_type = match ext.to_lowercase().as_str() {
                "png" | "jpg" | "jpeg" => "screenshot",
                "gif" => "gif",
                _ => continue,
            };

            let metadata = entry.metadata().ok();
            let modified = metadata.as_ref()
                .and_then(|m| m.modified().ok())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);
            let size = metadata.map(|m| m.len()).unwrap_or(0);

            files.push(FileInfo {
                path,
                filename,
                file_type: file_type.to_string(),
                modified,
                size,
            });
        }

        // Step 2: Sort by modified time descending
        files.sort_by(|a, b| b.modified.cmp(&a.modified));

        // Step 3: Apply filter if specified
        let filtered_files: Vec<_> = if let Some(ref ft) = filter_type {
            files.into_iter().filter(|f| &f.file_type == ft).collect()
        } else {
            files
        };

        // Step 4: Apply pagination
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(12);
        let total = filtered_files.len();
        let has_more = offset + limit < total;

        let page_files: Vec<_> = filtered_files.into_iter().skip(offset).take(limit).collect();

        // Step 5: Generate thumbnails only for the paginated subset
        let items: Vec<HistoryItem> = page_files
            .into_iter()
            .map(|f| {
                let thumbnail = generate_thumbnail(&f.path, &f.file_type);
                HistoryItem {
                    path: f.path.to_string_lossy().to_string(),
                    filename: f.filename,
                    file_type: f.file_type,
                    modified: f.modified,
                    size: f.size,
                    thumbnail,
                }
            })
            .collect();

        Ok(HistoryResponse { items, has_more, total })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_stats() -> Result<StatsResponse, String> {
    tokio::task::spawn_blocking(|| {
        let output_dir = dirs::picture_dir()
            .or_else(|| dirs::home_dir())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lovshot");

        if !output_dir.exists() {
            return Ok(StatsResponse {
                total_count: 0,
                screenshot_count: 0,
                gif_count: 0,
                total_size: 0,
                today_count: 0,
                week_count: 0,
            });
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let today_start = now - (now % 86400); // Start of today (UTC)
        let week_start = now - 7 * 86400;

        let entries = std::fs::read_dir(&output_dir).map_err(|e| e.to_string())?;

        let mut screenshot_count = 0usize;
        let mut gif_count = 0usize;
        let mut total_size = 0u64;
        let mut today_count = 0usize;
        let mut week_count = 0usize;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let is_screenshot = matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg");
            let is_gif = ext.to_lowercase() == "gif";

            if !is_screenshot && !is_gif {
                continue;
            }

            if is_screenshot {
                screenshot_count += 1;
            } else {
                gif_count += 1;
            }

            if let Ok(meta) = entry.metadata() {
                total_size += meta.len();
                if let Ok(modified) = meta.modified() {
                    let ts = modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                    if ts >= today_start {
                        today_count += 1;
                    }
                    if ts >= week_start {
                        week_count += 1;
                    }
                }
            }
        }

        Ok(StatsResponse {
            total_count: screenshot_count + gif_count,
            screenshot_count,
            gif_count,
            total_size,
            today_count,
            week_count,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
