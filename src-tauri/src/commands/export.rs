use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::thread;

use ab_glyph::{FontRef, PxScale};
use crate::capture::Screen;
use base64::{engine::general_purpose::STANDARD, Engine};
use gif::{Encoder, Frame, Repeat};
use image::{Rgba, RgbaImage};
use imageproc::drawing::draw_text_mut;
use tauri::{AppHandle, Emitter};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::config::WatermarkPosition;
use crate::state::SharedState;
use crate::types::{ExportConfig, ExportProgress, GifLoopMode, SaveResult, SizeEstimate};

// ============ Screenshot Watermark ============

/// Load system font for watermark
fn load_watermark_font() -> Option<FontRef<'static>> {
    #[cfg(target_os = "macos")]
    {
        let font_paths = [
            "/System/Library/Fonts/Menlo.ttc",
            "/System/Library/Fonts/Monaco.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
        ];
        for path in font_paths {
            if let Ok(data) = std::fs::read(path) {
                let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
                if let Ok(font) = FontRef::try_from_slice(leaked) {
                    return Some(font);
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let font_paths = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "C:\\Windows\\Fonts\\consola.ttf",
        ];
        for path in font_paths {
            if let Ok(data) = std::fs::read(path) {
                let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
                if let Ok(font) = FontRef::try_from_slice(leaked) {
                    return Some(font);
                }
            }
        }
    }
    None
}

/// Add #{number} watermark based on config position
fn add_screenshot_watermark(img: &mut RgbaImage, number: u64) {
    let cfg = crate::config::load_config();
    let position = cfg.watermark_position;

    // Skip if position is None or Brand (Brand is handled in share_compose)
    if matches!(position, WatermarkPosition::None | WatermarkPosition::Brand) {
        println!("[watermark] Position {:?}, skipping corner watermark", position);
        return;
    }

    let font = match load_watermark_font() {
        Some(f) => f,
        None => {
            println!("[watermark] Font not found, skipping");
            return;
        }
    };

    let text = format!("#{}", number);
    let (img_w, img_h) = img.dimensions();

    // Scale font size based on image dimensions
    let base_size = (img_w.min(img_h) as f32 * 0.04).clamp(14.0, 48.0);
    let scale = PxScale::from(base_size);

    // Estimate text dimensions
    let char_width = base_size * 0.6;
    let text_width = (text.len() as f32 * char_width) as u32;
    let text_height = base_size as u32;
    let padding = (base_size * 0.4) as u32;

    // Calculate position based on config
    let (x, y) = match position {
        WatermarkPosition::TopLeft => (padding as i32, padding as i32),
        WatermarkPosition::TopRight => ((img_w.saturating_sub(text_width + padding)) as i32, padding as i32),
        WatermarkPosition::BottomLeft => (padding as i32, (img_h.saturating_sub(text_height + padding)) as i32),
        WatermarkPosition::BottomRight => (
            (img_w.saturating_sub(text_width + padding)) as i32,
            (img_h.saturating_sub(text_height + padding)) as i32,
        ),
        _ => return,
    };

    // Draw with semi-transparent dark color
    let color = Rgba([40, 40, 40, 180]);
    draw_text_mut(img, color, x, y, scale, &font, &text);

    println!("[watermark] Added #{} at ({}, {}) position {:?}", number, x, y, position);
}

#[tauri::command]
pub fn estimate_export_size(
    state: tauri::State<SharedState>,
    config: ExportConfig,
) -> SizeEstimate {
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
    let estimated_bytes =
        (total_frames as f64 * output_width as f64 * output_height as f64 * bytes_per_pixel) as u64;
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
pub fn get_frame_thumbnail(
    state: tauri::State<SharedState>,
    frame_index: usize,
    max_height: u32,
) -> Result<String, String> {
    let s = state.lock().unwrap();

    if frame_index >= s.frames.len() {
        return Err("Frame index out of bounds".to_string());
    }

    let frame = &s.frames[frame_index];
    let (orig_w, orig_h) = frame.dimensions();

    let scale = max_height as f32 / orig_h as f32;
    let thumb_w = (orig_w as f32 * scale) as u32;
    let thumb_h = max_height;

    let thumbnail = image::imageops::resize(
        frame,
        thumb_w,
        thumb_h,
        image::imageops::FilterType::Triangle,
    );

    use image::ImageEncoder;
    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder
        .write_image(
            thumbnail.as_raw(),
            thumb_w,
            thumb_h,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;

    let base64_str = STANDARD.encode(&png_data);
    Ok(format!("data:image/png;base64,{}", base64_str))
}

#[tauri::command]
pub fn get_filmstrip(
    state: tauri::State<SharedState>,
    count: usize,
    thumb_height: u32,
) -> Result<Vec<String>, String> {
    let s = state.lock().unwrap();
    let total = s.frames.len();

    if total == 0 {
        return Err("No frames available".to_string());
    }

    let count = count.min(total).max(1);
    let step = if count > 1 {
        (total - 1) as f32 / (count - 1) as f32
    } else {
        0.0
    };

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

        let thumbnail = image::imageops::resize(
            frame,
            thumb_w,
            thumb_height,
            image::imageops::FilterType::Nearest,
        );

        let rgb_thumbnail = image::DynamicImage::ImageRgba8(thumbnail).to_rgb8();
        let mut jpg_data = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut jpg_data);
        rgb_thumbnail
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .map_err(|e| e.to_string())?;

        let base64_str = STANDARD.encode(&jpg_data);
        thumbnails.push(format!("data:image/jpeg;base64,{}", base64_str));
    }

    Ok(thumbnails)
}

#[tauri::command(rename_all = "camelCase")]
pub fn save_screenshot(
    app: AppHandle,
    state: tauri::State<SharedState>,
    scale: Option<f32>,
    use_cached: Option<bool>,
    caption_mode: Option<bool>,
) -> Result<String, String> {
    println!("[DEBUG][save_screenshot] ====== 被调用 ======");
    let s = state.lock().unwrap();
    let region = s.region.clone().ok_or("No region selected")?;
    let output_scale = scale.unwrap_or(1.0).clamp(0.1, 1.0);
    let cached_snapshot = s.cached_snapshot.clone();
    let screen_scale = s.screen_scale;
    let screen_x = s.screen_x;
    let screen_y = s.screen_y;
    let is_static_mode = use_cached.unwrap_or(false) && cached_snapshot.is_some();
    println!(
        "[DEBUG][save_screenshot] region: x={}, y={}, w={}, h={}, scale={}, static={}, caption_mode={:?}",
        region.x, region.y, region.width, region.height, output_scale, is_static_mode, caption_mode
    );
    drop(s);

    // Static mode: crop from cached snapshot
    // Dynamic mode: capture live screen
    let captured_rgba = if is_static_mode {
        if let Some(ref snapshot) = cached_snapshot {
            println!("[DEBUG][save_screenshot] 静态模式，从缓存截图裁剪");
            // Convert logical pixels to physical pixels
            let rel_x = ((region.x - screen_x) as f32 * screen_scale).max(0.0) as u32;
            let rel_y = ((region.y - screen_y) as f32 * screen_scale).max(0.0) as u32;
            let phys_w = (region.width as f32 * screen_scale) as u32;
            let phys_h = (region.height as f32 * screen_scale) as u32;

            // Clamp to valid bounds
            let max_x = snapshot.width().saturating_sub(1);
            let max_y = snapshot.height().saturating_sub(1);
            let crop_x = rel_x.min(max_x);
            let crop_y = rel_y.min(max_y);
            let crop_w = phys_w.min(snapshot.width().saturating_sub(crop_x));
            let crop_h = phys_h.min(snapshot.height().saturating_sub(crop_y));

            if crop_w == 0 || crop_h == 0 {
                return Err("Invalid capture area".to_string());
            }

            image::imageops::crop_imm(snapshot, crop_x, crop_y, crop_w, crop_h).to_image()
        } else {
            return Err("No cached snapshot for static mode".to_string());
        }
    } else {
        println!("[DEBUG][save_screenshot] 动态模式，实时截取屏幕");
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
        println!(
            "[DEBUG][save_screenshot] 调用 capture_area: x={}, y={}, w={}, h={}",
            region.x, region.y, region.width, region.height
        );
        let captured = screen
            .capture_area(region.x, region.y, region.width, region.height)
            .map_err(|e| {
                println!("[DEBUG][save_screenshot] capture_area 错误: {}", e);
                e.to_string()
            })?;
        println!(
            "[DEBUG][save_screenshot] capture_area 成功, 图像尺寸: {}x{}",
            captured.width(),
            captured.height()
        );

        RgbaImage::from_raw(captured.width(), captured.height(), captured.into_raw())
            .ok_or("Failed to convert image")?
    };

    let mut img = if (output_scale - 1.0).abs() > 0.01 {
        let new_w = (captured_rgba.width() as f32 * output_scale) as u32;
        let new_h = (captured_rgba.height() as f32 * output_scale) as u32;
        println!("[DEBUG][save_screenshot] 缩放到: {}x{}", new_w, new_h);
        image::imageops::resize(
            &captured_rgba,
            new_w,
            new_h,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        captured_rgba
    };

    // Add watermark with screenshot number
    let screenshot_number = crate::config::count_screenshots();
    add_screenshot_watermark(&mut img, screenshot_number);

    let tauri_image =
        tauri::image::Image::new_owned(img.as_raw().to_vec(), img.width(), img.height());
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

    let path_str = filename.to_string_lossy().to_string();
    let _ = app.emit("screenshot-saved", &path_str);

    // Show preview window: caption mode takes priority, then normal preview
    let is_caption_mode = caption_mode.unwrap_or(false);
    if is_caption_mode {
        println!("[save_screenshot] Opening caption preview window");
        if let Err(e) = crate::windows::open_caption_window(&app, &path_str) {
            println!("[save_screenshot] Failed to open caption window: {}", e);
        }
    } else {
        let cfg = crate::config::load_config();
        println!("[save_screenshot] screenshot_preview_enabled: {}", cfg.screenshot_preview_enabled);
        if cfg.screenshot_preview_enabled {
            if let Err(e) = crate::windows::open_preview_window(&app, &path_str) {
                println!("[save_screenshot] Failed to open preview: {}", e);
            }
        }
    }

    Ok(path_str)
}

#[tauri::command]
pub fn export_gif(
    app: AppHandle,
    state: tauri::State<SharedState>,
    config: ExportConfig,
) -> Result<(), String> {
    println!("[DEBUG][export_gif] ====== 被调用 ======");
    println!(
        "[DEBUG][export_gif] config: start={}, end={}, scale={}, fps={}, loop={}",
        config.start_frame,
        config.end_frame,
        config.output_scale,
        config.target_fps,
        config.loop_mode
    );

    let mut s = state.lock().unwrap();

    if s.frames.is_empty() {
        println!("[DEBUG][export_gif] 错误: 没有帧可保存");
        let _ = app.emit(
            "export-complete",
            SaveResult {
                success: false,
                path: None,
                error: Some("No frames to export".to_string()),
            },
        );
        return Ok(());
    }

    let total_frames = s.frames.len();
    let recording_fps = s.recording_fps;
    println!(
        "[DEBUG][export_gif] 原始帧数: {}, 录制帧率: {}",
        total_frames, recording_fps
    );

    let all_frames = s.frames.clone();
    drop(s);

    let config = config.clone();

    thread::spawn(move || {
        let start = config.start_frame.min(total_frames);
        let end = config.end_frame.min(total_frames);
        if end <= start {
            let _ = app.emit(
                "export-complete",
                SaveResult {
                    success: false,
                    path: None,
                    error: Some("Invalid frame range".to_string()),
                },
            );
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
                    let src_idx = (i as f32 * (trimmed_count - 1) as f32
                        / (target_frame_count - 1).max(1) as f32)
                        .round() as usize;
                    trimmed_frames[src_idx.min(trimmed_count - 1)].clone()
                })
                .collect()
        };
        println!(
            "[DEBUG][export_gif] 采样后: target={}, 实际={}, speed={}",
            target_frame_count,
            sampled_frames.len(),
            speed
        );

        if sampled_frames.is_empty() {
            let _ = app.emit(
                "export-complete",
                SaveResult {
                    success: false,
                    path: None,
                    error: Some("No frames after sampling".to_string()),
                },
            );
            return;
        }

        let output_scale = config.output_scale.clamp(0.1, 1.0);
        let scaled_frames: Vec<RgbaImage> = if (output_scale - 1.0).abs() > 0.01 {
            println!("[DEBUG][export_gif] 缩放帧: scale={}", output_scale);
            sampled_frames
                .into_iter()
                .map(|f| {
                    let new_w = (f.width() as f32 * output_scale) as u32;
                    let new_h = (f.height() as f32 * output_scale) as u32;
                    image::imageops::resize(&f, new_w, new_h, image::imageops::FilterType::Triangle)
                })
                .collect()
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
                let reversed: Vec<_> = scaled_frames[1..scaled_frames.len() - 1]
                    .iter()
                    .rev()
                    .cloned()
                    .collect();
                result.extend(reversed);
                println!(
                    "[DEBUG][export_gif] PingPong 模式: {} -> {} 帧",
                    scaled_frames.len(),
                    result.len()
                );
                result
            }
            _ => scaled_frames,
        };

        let output_dir = dirs::picture_dir()
            .or_else(|| dirs::home_dir())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lovshot");

        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            let _ = app.emit(
                "export-complete",
                SaveResult {
                    success: false,
                    path: None,
                    error: Some(e.to_string()),
                },
            );
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
        println!(
            "[DEBUG][export_gif] 开始编码: {}x{}, {} 帧",
            width, height, frame_count
        );

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
                let mut frame = Frame::from_rgba_speed(
                    width as u16,
                    height as u16,
                    &mut pixels,
                    gif_speed as i32,
                );
                frame.delay = delay;
                encoder.write_frame(&frame).map_err(|e| e.to_string())?;

                let _ = app.emit(
                    "export-progress",
                    ExportProgress {
                        current: i + 1,
                        total: frame_count,
                        stage: "encoding".to_string(),
                    },
                );

                if i == 0 || (i + 1) % 10 == 0 || i + 1 == frame_count {
                    println!("[DEBUG][export_gif] 编码帧 {}/{}", i + 1, frame_count);
                }
            }

            Ok(filename.to_string_lossy().to_string())
        })();

        match result {
            Ok(path) => {
                println!("[DEBUG][export_gif] ====== 完成 ====== 路径: {}", path);
                let _ = app.emit(
                    "export-complete",
                    SaveResult {
                        success: true,
                        path: Some(path),
                        error: None,
                    },
                );
            }
            Err(e) => {
                println!("[DEBUG][export_gif] ====== 错误 ====== {}", e);
                let _ = app.emit(
                    "export-complete",
                    SaveResult {
                        success: false,
                        path: None,
                        error: Some(e),
                    },
                );
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
    pub description: Option<String>, // Finder comment / caption
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

#[tauri::command]
pub async fn get_history(
    offset: Option<usize>,
    limit: Option<usize>,
    filter_type: Option<String>,
    folder: Option<String>,
) -> Result<HistoryResponse, String> {
    tokio::task::spawn_blocking(move || {
        let base_dir = get_lovshot_dir();

        // If folder specified, use that; otherwise use root (but only root-level files)
        let scan_dir = match &folder {
            Some(f) if !f.is_empty() => PathBuf::from(f),
            _ => base_dir.clone(),
        };

        if !scan_dir.exists() {
            return Ok(HistoryResponse {
                items: vec![],
                has_more: false,
                total: 0,
            });
        }

        // Step 1: Collect file metadata only (no thumbnail generation)
        let entries = std::fs::read_dir(&scan_dir).map_err(|e| e.to_string())?;
        let mut files: Vec<FileInfo> = vec![];

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let file_type = match ext.to_lowercase().as_str() {
                "png" | "jpg" | "jpeg" => "screenshot",
                "gif" => "gif",
                _ => continue,
            };

            let metadata = entry.metadata().ok();
            let modified = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                })
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

        let page_files: Vec<_> = filtered_files
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();

        // Step 5: Map to HistoryItem (no thumbnail generation - frontend uses original files)
        let items: Vec<HistoryItem> = page_files
            .into_iter()
            .map(|f| {
                let path_str = f.path.to_string_lossy().to_string();
                let description = read_finder_comment(&path_str);
                HistoryItem {
                    path: path_str,
                    filename: f.filename,
                    file_type: f.file_type,
                    modified: f.modified,
                    size: f.size,
                    thumbnail: String::new(),
                    description,
                }
            })
            .collect();

        Ok(HistoryResponse {
            items,
            has_more,
            total,
        })
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
                    let ts = modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
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

#[tauri::command]
pub fn save_caption(app: AppHandle, path: String, caption: String) -> Result<(), String> {
    println!("[save_caption] path: {}, caption: {}", path, caption);

    let input_path = PathBuf::from(&path);
    if !input_path.exists() {
        return Err(format!("File not found: {}", path));
    }

    // Write Finder comment in background, emit event when done
    let path_clone = path.clone();
    let caption_clone = caption.clone();
    let app_clone = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = write_png_comment(&path_clone, &caption_clone) {
            println!("[save_caption] PNG comment error: {}", e);
        } else {
            // Emit event to notify frontend
            let _ = app_clone.emit("caption-saved", serde_json::json!({
                "path": path_clone,
                "caption": caption_clone
            }));
            println!("[save_caption] Emitted caption-saved event");
        }
    });

    println!("[save_caption] Caption save initiated");

    // Close all caption windows from backend
    use tauri::Manager;
    for (label, window) in app.webview_windows() {
        if label.starts_with("caption-") {
            println!("[save_caption] Closing window: {}", label);
            let _ = window.destroy();
        }
    }

    Ok(())
}

fn write_png_comment(path: &str, comment: &str) -> Result<(), String> {
    // Set Finder comment using AppleScript via stdin (handles unicode properly)
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        // AppleScript to set Finder comment
        let script = format!(
            r#"set theFile to POSIX file "{}" as alias
tell application "Finder" to set comment of theFile to "{}""#,
            path.replace("\\", "\\\\").replace("\"", "\\\""),
            comment.replace("\\", "\\\\").replace("\"", "\\\"")
        );

        let mut child = Command::new("osascript")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("osascript spawn error: {}", e))?;

        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .map_err(|e| format!("osascript write error: {}", e))?;

        let output = child.wait_with_output().map_err(|e| format!("osascript error: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("AppleScript error: {}", stderr));
        }
    }

    println!("[save_caption] Finder comment set successfully");
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_finder_comment(path: &str) -> Option<String> {
    use std::path::Path;

    // Read directly from extended attribute (much faster than osascript)
    let xattr_name = "com.apple.metadata:kMDItemFinderComment";
    let path = Path::new(path);

    // Get xattr value as bytes
    let value = xattr::get(path, xattr_name).ok()??;

    // Parse plist to extract the string
    let plist: plist::Value = plist::from_bytes(&value).ok()?;
    let comment = plist.as_string()?.to_string();

    if comment.is_empty() {
        None
    } else {
        Some(comment)
    }
}

#[cfg(not(target_os = "macos"))]
fn read_finder_comment(_path: &str) -> Option<String> {
    None
}

#[tauri::command]
pub fn get_image_description(path: String) -> Option<String> {
    read_finder_comment(&path)
}

#[tauri::command]
pub fn delete_file(path: String) -> Result<(), String> {
    // Move to trash instead of permanent delete
    trash::delete(&path).map_err(|e| format!("Failed to delete file: {}", e))?;
    println!("[delete_file] Moved to trash: {}", path);
    Ok(())
}

// ============ Folder Management ============

#[derive(serde::Serialize)]
pub struct FolderInfo {
    pub name: String,
    pub path: String,
    pub file_count: usize,
}

fn get_lovshot_dir() -> PathBuf {
    dirs::picture_dir()
        .or_else(|| dirs::home_dir())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lovshot")
}

#[tauri::command]
pub fn get_folders() -> Result<Vec<FolderInfo>, String> {
    let base_dir = get_lovshot_dir();
    if !base_dir.exists() {
        return Ok(vec![]);
    }

    let entries = std::fs::read_dir(&base_dir).map_err(|e| e.to_string())?;
    let mut folders = vec![];

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Skip hidden folders
        if name.starts_with('.') {
            continue;
        }

        // Count image files in folder
        let file_count = std::fs::read_dir(&path)
            .map(|entries| {
                entries.flatten().filter(|e| {
                    let p = e.path();
                    if !p.is_file() { return false; }
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif")
                }).count()
            })
            .unwrap_or(0);

        folders.push(FolderInfo {
            name,
            path: path.to_string_lossy().to_string(),
            file_count,
        });
    }

    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(folders)
}

#[tauri::command]
pub fn create_folder(name: String) -> Result<FolderInfo, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("Folder name cannot contain path separators".to_string());
    }

    let base_dir = get_lovshot_dir();
    std::fs::create_dir_all(&base_dir).map_err(|e| e.to_string())?;

    let folder_path = base_dir.join(name);
    if folder_path.exists() {
        return Err("Folder already exists".to_string());
    }

    std::fs::create_dir(&folder_path).map_err(|e| e.to_string())?;
    println!("[create_folder] Created: {:?}", folder_path);

    Ok(FolderInfo {
        name: name.to_string(),
        path: folder_path.to_string_lossy().to_string(),
        file_count: 0,
    })
}

#[tauri::command]
pub fn delete_folder(path: String) -> Result<(), String> {
    let folder_path = PathBuf::from(&path);
    if !folder_path.exists() {
        return Err("Folder not found".to_string());
    }
    if !folder_path.is_dir() {
        return Err("Not a folder".to_string());
    }

    // Check if folder is empty (except hidden files)
    let has_files = std::fs::read_dir(&folder_path)
        .map(|entries| {
            entries.flatten().any(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                !name.starts_with('.')
            })
        })
        .unwrap_or(false);

    if has_files {
        return Err("Folder is not empty".to_string());
    }

    std::fs::remove_dir_all(&folder_path).map_err(|e| e.to_string())?;
    println!("[delete_folder] Deleted: {:?}", folder_path);
    Ok(())
}

#[tauri::command]
pub fn rename_folder(path: String, new_name: String) -> Result<FolderInfo, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }
    if new_name.contains('/') || new_name.contains('\\') {
        return Err("Folder name cannot contain path separators".to_string());
    }

    let old_path = PathBuf::from(&path);
    if !old_path.exists() || !old_path.is_dir() {
        return Err("Folder not found".to_string());
    }

    let parent = old_path.parent().ok_or("Invalid folder path")?;
    let new_path = parent.join(new_name);

    if new_path.exists() {
        return Err("A folder with this name already exists".to_string());
    }

    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;
    println!("[rename_folder] Renamed {:?} -> {:?}", old_path, new_path);

    let file_count = std::fs::read_dir(&new_path)
        .map(|entries| {
            entries.flatten().filter(|e| {
                let p = e.path();
                if !p.is_file() { return false; }
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif")
            }).count()
        })
        .unwrap_or(0);

    Ok(FolderInfo {
        name: new_name.to_string(),
        path: new_path.to_string_lossy().to_string(),
        file_count,
    })
}

#[tauri::command]
pub fn move_to_folder(file_paths: Vec<String>, folder_path: Option<String>) -> Result<Vec<String>, String> {
    let base_dir = get_lovshot_dir();
    let target_dir = match folder_path {
        Some(ref p) if !p.is_empty() => PathBuf::from(p),
        _ => base_dir.clone(), // Move to root if no folder specified
    };

    if !target_dir.exists() {
        return Err("Target folder not found".to_string());
    }

    let mut new_paths = vec![];
    for file_path in file_paths {
        let src = PathBuf::from(&file_path);
        if !src.exists() {
            continue;
        }

        let filename = src.file_name().ok_or("Invalid filename")?;
        let dest = target_dir.join(filename);

        // Skip if already in target folder
        if src == dest {
            new_paths.push(file_path);
            continue;
        }

        // Handle name conflict
        let final_dest = if dest.exists() {
            let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("");
            let mut counter = 1;
            loop {
                let new_name = if ext.is_empty() {
                    format!("{}_{}", stem, counter)
                } else {
                    format!("{}_{}.{}", stem, counter, ext)
                };
                let candidate = target_dir.join(&new_name);
                if !candidate.exists() {
                    break candidate;
                }
                counter += 1;
            }
        } else {
            dest
        };

        std::fs::rename(&src, &final_dest).map_err(|e| e.to_string())?;
        println!("[move_to_folder] Moved {:?} -> {:?}", src, final_dest);
        new_paths.push(final_dest.to_string_lossy().to_string());
    }

    Ok(new_paths)
}

/// Preview export content without saving
#[tauri::command]
pub fn preview_folder_export(folder_path: Option<String>, format: String) -> Result<String, String> {
    let base_dir = get_lovshot_dir();
    let scan_dir = match &folder_path {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => base_dir.clone(),
    };

    if !scan_dir.exists() {
        return Err("Folder not found".to_string());
    }

    // Collect image files
    let entries = std::fs::read_dir(&scan_dir).map_err(|e| e.to_string())?;
    let mut files: Vec<(PathBuf, String, u64)> = vec![];

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif") {
            continue;
        }

        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let modified = entry.metadata()
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
            .unwrap_or(0);

        files.push((path, filename, modified));
    }

    // Sort by modified time ascending (oldest first)
    files.sort_by(|a, b| a.2.cmp(&b.2));

    // Generate content
    let mut lines: Vec<String> = vec![];

    for (path, filename, _) in &files {
        let caption = read_finder_comment(&path.to_string_lossy());
        let alt_text = caption.as_deref().unwrap_or(filename);
        let path_str = path.to_string_lossy();

        let image_ref = match format.as_str() {
            "writing" => format!("{}：\n\n![]({})", alt_text, path_str),
            "html" => format!("<img src=\"{}\" alt=\"{}\" />", path_str, alt_text),
            "url_only" => path_str.to_string(),
            _ => format!("![{}]({})", alt_text, path_str),
        };

        lines.push(image_ref);
    }

    Ok(lines.join("\n\n"))
}

#[tauri::command]
pub fn export_folder_to_md(folder_path: Option<String>, format: String) -> Result<String, String> {
    let base_dir = get_lovshot_dir();
    let scan_dir = match &folder_path {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => base_dir.clone(),
    };

    if !scan_dir.exists() {
        return Err("Folder not found".to_string());
    }

    let folder_name = scan_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("lovshot");

    // Generate content using preview function
    let md_content = preview_folder_export(folder_path, format)?;

    // Save to folder
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let md_filename = scan_dir.join(format!("{}_export_{}.md", folder_name, timestamp));
    std::fs::write(&md_filename, &md_content).map_err(|e| e.to_string())?;

    println!("[export_folder_to_md] Exported to {:?}", md_filename);
    Ok(md_filename.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_caption_editor(app: AppHandle, path: String) -> Result<(), String> {
    crate::windows::open_caption_window(&app, &path)
}

#[tauri::command]
pub fn copy_image_to_clipboard(app: AppHandle, path: String) -> Result<(), String> {
    let img = image::open(&path).map_err(|e| format!("Failed to open image: {}", e))?;
    let rgba = img.to_rgba8();
    let tauri_image = tauri::image::Image::new_owned(rgba.as_raw().to_vec(), rgba.width(), rgba.height());
    app.clipboard().write_image(&tauri_image).map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn copy_rgba_to_clipboard(app: AppHandle, data: Vec<u8>, width: u32, height: u32) -> Result<(), String> {
    println!("[copy_rgba_to_clipboard] {}x{}, data len: {}", width, height, data.len());
    let tauri_image = tauri::image::Image::new_owned(data, width, height);
    app.clipboard().write_image(&tauri_image).map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
    println!("[copy_rgba_to_clipboard] Success");
    Ok(())
}

#[tauri::command]
pub fn save_rgba_to_file(data: Vec<u8>, width: u32, height: u32, path: String) -> Result<(), String> {
    println!("[save_rgba_to_file] {}x{} -> {}", width, height, path);
    let img = image::RgbaImage::from_raw(width, height, data)
        .ok_or("Failed to create image from RGBA data")?;
    img.save(&path).map_err(|e| format!("Failed to save image: {}", e))?;
    println!("[save_rgba_to_file] Success");
    Ok(())
}

/// Capture a region and return base64 PNG for annotation editing
#[tauri::command(rename_all = "camelCase")]
pub fn capture_region_preview(
    state: tauri::State<SharedState>,
    region: crate::types::Region,
) -> Result<String, String> {
    println!("[capture_region_preview] region: x={}, y={}, w={}, h={}",
        region.x, region.y, region.width, region.height);

    let s = state.lock().unwrap();
    let cached_snapshot = s.cached_snapshot.clone();
    let screen_scale = s.screen_scale;
    let screen_x = s.screen_x;
    let screen_y = s.screen_y;
    drop(s);

    // Try to use cached snapshot first (for static mode)
    let captured_rgba = if let Some(ref snapshot) = cached_snapshot {
        println!("[capture_region_preview] Using cached snapshot");
        // Convert logical pixels to physical pixels
        let rel_x = ((region.x - screen_x) as f32 * screen_scale).max(0.0) as u32;
        let rel_y = ((region.y - screen_y) as f32 * screen_scale).max(0.0) as u32;
        let phys_w = (region.width as f32 * screen_scale) as u32;
        let phys_h = (region.height as f32 * screen_scale) as u32;

        let max_x = snapshot.width().saturating_sub(1);
        let max_y = snapshot.height().saturating_sub(1);
        let crop_x = rel_x.min(max_x);
        let crop_y = rel_y.min(max_y);
        let crop_w = phys_w.min(snapshot.width().saturating_sub(crop_x));
        let crop_h = phys_h.min(snapshot.height().saturating_sub(crop_y));

        if crop_w == 0 || crop_h == 0 {
            return Err("Invalid capture area".to_string());
        }

        image::imageops::crop_imm(snapshot, crop_x, crop_y, crop_w, crop_h).to_image()
    } else {
        // Capture live screen
        println!("[capture_region_preview] Capturing live screen");
        let screens = Screen::all().map_err(|e| e.to_string())?;
        if screens.is_empty() {
            return Err("No screens found".to_string());
        }

        let screen = &screens[0];
        let captured = screen
            .capture_area(region.x, region.y, region.width, region.height)
            .map_err(|e| e.to_string())?;

        RgbaImage::from_raw(captured.width(), captured.height(), captured.into_raw())
            .ok_or("Failed to convert image")?
    };

    // Encode to PNG base64
    use image::ImageEncoder;
    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder.write_image(
        captured_rgba.as_raw(),
        captured_rgba.width(),
        captured_rgba.height(),
        image::ExtendedColorType::Rgba8,
    ).map_err(|e| e.to_string())?;

    let base64_str = STANDARD.encode(&png_data);
    Ok(format!("data:image/png;base64,{}", base64_str))
}

/// Save an annotated screenshot from base64 PNG data
#[tauri::command(rename_all = "camelCase")]
pub fn save_annotated_screenshot(
    app: AppHandle,
    image_data: String,
    caption_mode: Option<bool>,
) -> Result<String, String> {
    println!("[save_annotated_screenshot] Saving annotated screenshot, caption_mode={:?}", caption_mode);

    // Decode base64
    let decoded = STANDARD.decode(&image_data)
        .map_err(|e| format!("Base64 decode error: {}", e))?;

    // Load as image
    let mut img = image::load_from_memory(&decoded)
        .map_err(|e| format!("Image load error: {}", e))?
        .to_rgba8();

    println!("[save_annotated_screenshot] Image size: {}x{}", img.width(), img.height());

    // Add watermark with screenshot number
    let screenshot_number = crate::config::count_screenshots();
    add_screenshot_watermark(&mut img, screenshot_number);

    // Copy to clipboard
    let tauri_image = tauri::image::Image::new_owned(
        img.as_raw().to_vec(),
        img.width(),
        img.height()
    );
    app.clipboard().write_image(&tauri_image)
        .map_err(|e| format!("Clipboard error: {}", e))?;
    println!("[save_annotated_screenshot] Copied to clipboard");

    // Save to file
    let output_dir = dirs::picture_dir()
        .or_else(|| dirs::home_dir())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lovshot");

    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = output_dir.join(format!("screenshot_{}.png", timestamp));

    img.save(&filename).map_err(|e| format!("Save error: {}", e))?;
    println!("[save_annotated_screenshot] Saved to {:?}", filename);

    let path_str = filename.to_string_lossy().to_string();
    let _ = app.emit("screenshot-saved", &path_str);

    // Show preview window
    let is_caption_mode = caption_mode.unwrap_or(false);
    if is_caption_mode {
        if let Err(e) = crate::windows::open_caption_window(&app, &path_str) {
            println!("[save_annotated_screenshot] Failed to open caption window: {}", e);
        }
    } else {
        let cfg = crate::config::load_config();
        if cfg.screenshot_preview_enabled {
            if let Err(e) = crate::windows::open_preview_window(&app, &path_str) {
                println!("[save_annotated_screenshot] Failed to open preview: {}", e);
            }
        }
    }

    Ok(path_str)
}
