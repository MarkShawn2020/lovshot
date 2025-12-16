use std::path::PathBuf;

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{RgbaImage, GenericImage, DynamicImage};
use screenshots::Screen;
use tauri::{AppHandle, Manager, WebviewWindowBuilder, WebviewUrl, PhysicalPosition, PhysicalSize};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::state::SharedState;
use crate::types::{ScrollCaptureProgress, Region, CropEdges};

/// Start scroll capture mode - captures the initial frame
#[tauri::command]
pub fn start_scroll_capture(state: tauri::State<SharedState>) -> Result<ScrollCaptureProgress, String> {
    println!("[DEBUG][start_scroll_capture] ====== 被调用 ======");
    let mut s = state.lock().unwrap();
    let region = s.region.clone().ok_or_else(|| {
        println!("[DEBUG][start_scroll_capture] 错误: No region selected");
        "No region selected".to_string()
    })?;
    println!("[DEBUG][start_scroll_capture] region: x={}, y={}, w={}, h={}",
        region.x, region.y, region.width, region.height);

    // Clear previous scroll capture state
    s.scroll_frames.clear();
    s.scroll_offsets.clear();
    s.scroll_stitched = None;
    s.scroll_capturing = true;

    drop(s);

    // Capture initial frame
    println!("[DEBUG][start_scroll_capture] 开始截图...");
    let screens = Screen::all().map_err(|e| {
        println!("[DEBUG][start_scroll_capture] Screen::all 错误: {}", e);
        e.to_string()
    })?;
    if screens.is_empty() {
        println!("[DEBUG][start_scroll_capture] 错误: No screens found");
        return Err("No screens found".to_string());
    }
    println!("[DEBUG][start_scroll_capture] 找到 {} 个屏幕", screens.len());

    let screen = &screens[0];
    let captured = screen.capture_area(region.x, region.y, region.width, region.height)
        .map_err(|e| {
            println!("[DEBUG][start_scroll_capture] capture_area 错误: {}", e);
            e.to_string()
        })?;
    println!("[DEBUG][start_scroll_capture] 截图成功: {}x{}", captured.width(), captured.height());

    let frame = RgbaImage::from_raw(
        captured.width(),
        captured.height(),
        captured.into_raw(),
    ).ok_or("Failed to convert image")?;

    let (_width, height) = frame.dimensions();

    // Store initial frame
    let mut s = state.lock().unwrap();
    s.scroll_frames.push(frame.clone());
    s.scroll_offsets.push(0);
    s.scroll_stitched = Some(frame.clone());

    // Generate preview
    println!("[DEBUG][start_scroll_capture] 生成预览...");
    let preview = generate_preview_base64(&frame, 200)?;
    println!("[DEBUG][start_scroll_capture] 完成! frame_count=1, height={}", height);

    Ok(ScrollCaptureProgress {
        frame_count: 1,
        total_height: height,
        preview_base64: preview,
    })
}

/// Auto-detect scroll by comparing current frame with previous frame
/// Returns None if no significant change detected
#[tauri::command]
pub fn capture_scroll_frame_auto(
    state: tauri::State<SharedState>,
) -> Result<Option<ScrollCaptureProgress>, String> {
    let region = {
        let s = state.lock().unwrap();
        if !s.scroll_capturing {
            return Err("Not in scroll capture mode".to_string());
        }
        s.region.clone().ok_or("No region selected")?
    };

    // Capture current frame
    let screens = Screen::all().map_err(|e| e.to_string())?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let screen = &screens[0];
    let captured = screen.capture_area(region.x, region.y, region.width, region.height)
        .map_err(|e| e.to_string())?;

    let new_frame = RgbaImage::from_raw(
        captured.width(),
        captured.height(),
        captured.into_raw(),
    ).ok_or("Failed to convert image")?;

    let mut s = state.lock().unwrap();

    // Get last frame for comparison
    let last_frame = s.scroll_frames.last().ok_or("No previous frame")?;

    // Detect scroll direction and amount by comparing frames
    let scroll_delta = detect_scroll_delta(last_frame, &new_frame);

    // If no significant scroll detected, return current progress without changes
    if scroll_delta.abs() < 10 {
        if let Some(ref stitched) = s.scroll_stitched {
            let preview = generate_preview_base64(stitched, 300)?;
            return Ok(Some(ScrollCaptureProgress {
                frame_count: s.scroll_frames.len(),
                total_height: stitched.height(),
                preview_base64: preview,
            }));
        }
        return Ok(None);
    }

    // Stitch the image
    let stitched = stitch_scroll_image(
        s.scroll_stitched.as_ref().unwrap(),
        &new_frame,
        scroll_delta,
    )?;

    // Calculate new cumulative offset
    let last_offset = *s.scroll_offsets.last().unwrap_or(&0);
    let new_offset = last_offset + scroll_delta;

    s.scroll_frames.push(new_frame);
    s.scroll_offsets.push(new_offset);
    s.scroll_stitched = Some(stitched.clone());

    let frame_count = s.scroll_frames.len();
    let total_height = stitched.height();

    // Generate preview
    let preview = generate_preview_base64(&stitched, 300)?;

    Ok(Some(ScrollCaptureProgress {
        frame_count,
        total_height,
        preview_base64: preview,
    }))
}

/// Detect scroll amount by comparing two frames
/// Returns positive for scroll down, negative for scroll up
fn detect_scroll_delta(prev: &RgbaImage, curr: &RgbaImage) -> i32 {
    let (w, h) = prev.dimensions();
    let (w2, h2) = curr.dimensions();

    if w != w2 || h != h2 {
        return 0;
    }

    let h = h as i32;
    let search_range = (h / 2).min(200); // Search up to half height or 200px

    // Try to find where current frame's top matches in previous frame
    // This tells us how much was scrolled down
    let mut best_match_down = 0;
    let mut best_score_down = i64::MAX;

    // Try to find where current frame's bottom matches in previous frame
    // This tells us how much was scrolled up
    let mut best_match_up = 0;
    let mut best_score_up = i64::MAX;

    let strip_height = 20; // Compare strips of this height

    for offset in (10..search_range).step_by(5) {
        // Check scroll down: current top should match previous middle/bottom
        let score_down = compare_strips(prev, curr, offset as u32, 0, w, strip_height);
        if score_down < best_score_down {
            best_score_down = score_down;
            best_match_down = offset;
        }

        // Check scroll up: current bottom should match previous middle/top
        let score_up = compare_strips(prev, curr, 0, offset as u32, w, strip_height);
        if score_up < best_score_up {
            best_score_up = score_up;
            best_match_up = offset;
        }
    }

    // Threshold for considering it a match (lower is better)
    let threshold = (w as i64) * (strip_height as i64) * 50; // Allow some variation

    if best_score_down < threshold && best_score_down <= best_score_up {
        best_match_down // Scrolled down
    } else if best_score_up < threshold {
        -best_match_up // Scrolled up
    } else {
        0 // No clear scroll detected
    }
}

/// Compare horizontal strips from two images
/// Returns sum of absolute differences (lower = more similar)
fn compare_strips(prev: &RgbaImage, curr: &RgbaImage, prev_y: u32, curr_y: u32, width: u32, height: u32) -> i64 {
    let mut diff: i64 = 0;
    let (_, prev_h) = prev.dimensions();
    let (_, curr_h) = curr.dimensions();

    if prev_y + height > prev_h || curr_y + height > curr_h {
        return i64::MAX;
    }

    // Sample every 4th pixel for speed
    for y in 0..height {
        for x in (0..width).step_by(4) {
            let p1 = prev.get_pixel(x, prev_y + y);
            let p2 = curr.get_pixel(x, curr_y + y);

            diff += (p1[0] as i64 - p2[0] as i64).abs();
            diff += (p1[1] as i64 - p2[1] as i64).abs();
            diff += (p1[2] as i64 - p2[2] as i64).abs();
        }
    }
    diff
}

/// Get current scroll preview without capturing new frame
#[tauri::command]
pub fn get_scroll_preview(state: tauri::State<SharedState>) -> Result<ScrollCaptureProgress, String> {
    let s = state.lock().unwrap();

    if let Some(ref stitched) = s.scroll_stitched {
        let preview = generate_preview_base64(stitched, 300)?;
        Ok(ScrollCaptureProgress {
            frame_count: s.scroll_frames.len(),
            total_height: stitched.height(),
            preview_base64: preview,
        })
    } else {
        Err("No scroll capture in progress".to_string())
    }
}

/// Copy scroll capture to clipboard
#[tauri::command]
pub fn copy_scroll_to_clipboard(app: AppHandle, state: tauri::State<SharedState>, crop: Option<CropEdges>) -> Result<(), String> {
    let s = state.lock().unwrap();
    let stitched = s.scroll_stitched.as_ref().ok_or("No stitched image")?;

    let final_img = apply_crop(stitched, crop)?;

    let tauri_image = tauri::image::Image::new_owned(
        final_img.as_raw().to_vec(),
        final_img.width(),
        final_img.height(),
    );
    app.clipboard().write_image(&tauri_image).map_err(|e| e.to_string())?;

    Ok(())
}

/// Finish scroll capture - save the stitched image to specified path
#[tauri::command]
pub fn finish_scroll_capture(state: tauri::State<SharedState>, path: String, crop: Option<CropEdges>) -> Result<String, String> {
    let mut s = state.lock().unwrap();

    if !s.scroll_capturing {
        return Err("Not in scroll capture mode".to_string());
    }

    let stitched = s.scroll_stitched.take().ok_or("No stitched image")?;

    // Clear scroll state
    s.scroll_capturing = false;
    s.scroll_frames.clear();
    s.scroll_offsets.clear();

    drop(s);

    // Apply crop and save
    let final_img = apply_crop(&stitched, crop)?;
    final_img.save(&path).map_err(|e| e.to_string())?;

    Ok(path)
}

/// Cancel scroll capture
#[tauri::command]
pub fn cancel_scroll_capture(state: tauri::State<SharedState>) {
    let mut s = state.lock().unwrap();
    s.scroll_capturing = false;
    s.scroll_frames.clear();
    s.scroll_offsets.clear();
    s.scroll_stitched = None;
}

/// Stitch two images based on scroll delta
/// scroll_delta > 0: scrolled down, new content at bottom
/// scroll_delta < 0: scrolled up, new content at top
fn stitch_scroll_image(
    base: &RgbaImage,
    new_frame: &RgbaImage,
    scroll_delta: i32,
) -> Result<RgbaImage, String> {
    let (base_w, base_h) = base.dimensions();
    let (new_w, new_h) = new_frame.dimensions();

    // Ensure same width
    if base_w != new_w {
        return Err("Frame width mismatch".to_string());
    }

    let abs_delta = scroll_delta.abs() as u32;

    if scroll_delta > 0 {
        // Scrolled down: append new content at bottom
        // The overlap is (new_h - abs_delta) pixels
        // We only add the non-overlapping part of new_frame

        if abs_delta >= new_h {
            // No overlap, just concatenate
            let new_height = base_h + new_h;
            let mut result = RgbaImage::new(base_w, new_height);
            result.copy_from(base, 0, 0).map_err(|e| e.to_string())?;
            result.copy_from(new_frame, 0, base_h).map_err(|e| e.to_string())?;
            Ok(result)
        } else {
            // Has overlap, only add new pixels
            let pixels_to_add = abs_delta.min(new_h);
            let new_height = base_h + pixels_to_add;
            let mut result = RgbaImage::new(base_w, new_height);

            // Copy base image
            result.copy_from(base, 0, 0).map_err(|e| e.to_string())?;

            // Copy only the new (bottom) part of new_frame
            let crop_y = new_h - pixels_to_add;
            let cropped = DynamicImage::ImageRgba8(new_frame.clone())
                .crop_imm(0, crop_y, new_w, pixels_to_add)
                .to_rgba8();
            result.copy_from(&cropped, 0, base_h).map_err(|e| e.to_string())?;

            Ok(result)
        }
    } else {
        // Scrolled up: prepend new content at top
        if abs_delta >= new_h {
            // No overlap, just concatenate
            let new_height = new_h + base_h;
            let mut result = RgbaImage::new(base_w, new_height);
            result.copy_from(new_frame, 0, 0).map_err(|e| e.to_string())?;
            result.copy_from(base, 0, new_h).map_err(|e| e.to_string())?;
            Ok(result)
        } else {
            // Has overlap, only add new pixels at top
            let pixels_to_add = abs_delta.min(new_h);
            let new_height = base_h + pixels_to_add;
            let mut result = RgbaImage::new(base_w, new_height);

            // Copy only the new (top) part of new_frame
            let cropped = DynamicImage::ImageRgba8(new_frame.clone())
                .crop_imm(0, 0, new_w, pixels_to_add)
                .to_rgba8();
            result.copy_from(&cropped, 0, 0).map_err(|e| e.to_string())?;

            // Copy base image below the new content
            result.copy_from(base, 0, pixels_to_add).map_err(|e| e.to_string())?;

            Ok(result)
        }
    }
}

/// Apply percentage-based edge crop to an image
fn apply_crop(img: &RgbaImage, crop: Option<CropEdges>) -> Result<RgbaImage, String> {
    let crop = match crop {
        Some(c) if c.top > 0.0 || c.bottom > 0.0 || c.left > 0.0 || c.right > 0.0 => c,
        _ => return Ok(img.clone()), // No crop
    };

    let (w, h) = img.dimensions();

    // Convert percentage to pixels
    let top_px = ((crop.top / 100.0) * h as f32).round() as u32;
    let bottom_px = ((crop.bottom / 100.0) * h as f32).round() as u32;
    let left_px = ((crop.left / 100.0) * w as f32).round() as u32;
    let right_px = ((crop.right / 100.0) * w as f32).round() as u32;

    // Validate
    if left_px + right_px >= w || top_px + bottom_px >= h {
        return Err("Crop exceeds image bounds".to_string());
    }

    let new_w = w - left_px - right_px;
    let new_h = h - top_px - bottom_px;

    let cropped = DynamicImage::ImageRgba8(img.clone())
        .crop_imm(left_px, top_px, new_w, new_h)
        .to_rgba8();
    Ok(cropped)
}

/// Generate a preview image as base64 JPEG (fast), scaled to fit max_height
fn generate_preview_base64(img: &RgbaImage, max_height: u32) -> Result<String, String> {
    let (w, h) = img.dimensions();

    // Use faster Nearest filter and smaller preview for speed
    let preview = if h > max_height {
        let scale = max_height as f32 / h as f32;
        let new_w = (w as f32 * scale).max(1.0) as u32;
        image::imageops::resize(img, new_w, max_height, image::imageops::FilterType::Nearest)
    } else {
        img.clone()
    };

    // Convert RGBA to RGB for JPEG (faster than PNG)
    let rgb_preview = DynamicImage::ImageRgba8(preview).to_rgb8();

    let mut jpg_data = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpg_data);
    rgb_preview.write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;

    let base64_str = STANDARD.encode(&jpg_data);
    Ok(format!("data:image/jpeg;base64,{}", base64_str))
}

/// Open the scroll overlay window (non-activating panel on macOS)
/// This window won't steal focus, allowing scroll events to pass to underlying windows
#[tauri::command]
pub fn open_scroll_overlay(app: AppHandle, state: tauri::State<SharedState>, region: Region) -> Result<(), String> {
    println!("[DEBUG][open_scroll_overlay] 打开滚动截图悬浮窗");

    // Close existing scroll-overlay if any
    if let Some(win) = app.get_webview_window("scroll-overlay") {
        let _ = win.close();
    }

    // Get screen info for positioning
    let screens = Screen::all().map_err(|e| e.to_string())?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let screen = &screens[0];
    let scale = screen.display_info.scale_factor;

    // Position the overlay to the right of the selection region
    let panel_width = 220.0;
    let panel_height = 400.0;
    let margin = 12.0;

    // Calculate position: prefer right side, fallback to left
    let screen_width = screen.display_info.width as f32;
    let region_right = region.x as f32 + region.width as f32;
    let right_space = screen_width - region_right;

    let panel_x = if right_space >= panel_width + margin {
        region_right + margin
    } else {
        (region.x as f32 - panel_width - margin).max(0.0)
    };
    let panel_y = region.y as f32;

    // Store region for capture
    {
        let mut s = state.lock().unwrap();
        s.region = Some(region);
    }

    let win = WebviewWindowBuilder::new(&app, "scroll-overlay", WebviewUrl::App("/scroll-overlay.html".into()))
        .title("Lovshot Scroll")
        .inner_size(panel_width as f64, panel_height as f64)
        .min_inner_size(280.0, 200.0)
        .position(panel_x as f64, panel_y as f64)
        .decorations(false)
        .resizable(true)
        .always_on_top(true)
        .focused(true)
        .transparent(true)
        .build()
        .map_err(|e| e.to_string())?;

    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())?;

    println!("[DEBUG][open_scroll_overlay] 悬浮窗创建成功");
    Ok(())
}
