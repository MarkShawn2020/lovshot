use std::path::PathBuf;

use crate::capture::Screen;
use base64::{engine::general_purpose::STANDARD, Engine};
use image::codecs::jpeg::JpegEncoder;
use image::ExtendedColorType;
use image::{DynamicImage, GenericImage, RgbaImage};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::fft_match::detect_scroll_delta_fft;
use crate::state::SharedState;
use crate::tray::create_recording_overlay;
use crate::types::{CropEdges, Region, ScrollCaptureProgress};

/// Start scroll capture mode - captures the initial frame
#[tauri::command]
pub fn start_scroll_capture(
    state: tauri::State<SharedState>,
) -> Result<ScrollCaptureProgress, String> {
    println!("[DEBUG][start_scroll_capture] ====== 被调用 ======");
    let mut s = state.lock().unwrap();
    let region = s.region.clone().ok_or_else(|| {
        println!("[DEBUG][start_scroll_capture] 错误: No region selected");
        "No region selected".to_string()
    })?;
    println!(
        "[DEBUG][start_scroll_capture] region: x={}, y={}, w={}, h={}",
        region.x, region.y, region.width, region.height
    );

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
    println!(
        "[DEBUG][start_scroll_capture] 找到 {} 个屏幕",
        screens.len()
    );

    let screen = &screens[0];
    let captured = screen
        .capture_area(region.x, region.y, region.width, region.height)
        .map_err(|e| {
            println!("[DEBUG][start_scroll_capture] capture_area 错误: {}", e);
            e.to_string()
        })?;
    println!(
        "[DEBUG][start_scroll_capture] 截图成功: {}x{}",
        captured.width(),
        captured.height()
    );

    let frame = RgbaImage::from_raw(captured.width(), captured.height(), captured.into_raw())
        .ok_or("Failed to convert image")?;

    let (_width, height) = frame.dimensions();

    // Store initial frame
    let mut s = state.lock().unwrap();
    s.scroll_frames.push(frame.clone());
    s.scroll_offsets.push(0);
    s.scroll_stitched = Some(frame.clone());

    // Generate preview
    println!("[DEBUG][start_scroll_capture] 生成预览...");
    let preview = generate_preview_base64(&frame, 600)?;
    println!(
        "[DEBUG][start_scroll_capture] 完成! frame_count=1, height={}",
        height
    );

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
    let captured = screen
        .capture_area(region.x, region.y, region.width, region.height)
        .map_err(|e| e.to_string())?;

    let new_frame = RgbaImage::from_raw(captured.width(), captured.height(), captured.into_raw())
        .ok_or("Failed to convert image")?;

    let mut s = state.lock().unwrap();

    // Get last frame for comparison
    let last_frame = s.scroll_frames.last().ok_or("No previous frame")?;

    // Detect scroll direction and amount using FFT-based matching
    let scroll_delta = detect_scroll_delta_fft(last_frame, &new_frame);

    // If no significant scroll detected, don't refresh preview (keeps UI stable)
    if scroll_delta.abs() < 10 {
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
    let preview = generate_preview_base64(&stitched, 600)?;

    Ok(Some(ScrollCaptureProgress {
        frame_count,
        total_height,
        preview_base64: preview,
    }))
}

/// Get current scroll preview without capturing new frame
#[tauri::command]
pub fn get_scroll_preview(
    state: tauri::State<SharedState>,
) -> Result<ScrollCaptureProgress, String> {
    let s = state.lock().unwrap();

    if let Some(ref stitched) = s.scroll_stitched {
        let preview = generate_preview_base64(stitched, 600)?;
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
pub fn copy_scroll_to_clipboard(
    app: AppHandle,
    state: tauri::State<SharedState>,
    crop: Option<CropEdges>,
) -> Result<(), String> {
    let s = state.lock().unwrap();
    let stitched = s.scroll_stitched.as_ref().ok_or("No stitched image")?;

    let final_img = apply_crop(stitched, crop)?;

    let tauri_image = tauri::image::Image::new_owned(
        final_img.as_raw().to_vec(),
        final_img.width(),
        final_img.height(),
    );
    app.clipboard()
        .write_image(&tauri_image)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Finish scroll capture - save the stitched image to specified path
#[tauri::command]
pub fn finish_scroll_capture(
    app: AppHandle,
    state: tauri::State<SharedState>,
    path: String,
    crop: Option<CropEdges>,
) -> Result<String, String> {
    let mut s = state.lock().unwrap();
    let stitched = s.scroll_stitched.take().ok_or("No stitched image")?;

    // Clear scroll state
    s.scroll_capturing = false;
    s.scroll_frames.clear();
    s.scroll_offsets.clear();

    drop(s);

    // Apply crop and save
    let final_img = apply_crop(&stitched, crop)?;
    final_img.save(&path).map_err(|e| e.to_string())?;

    // Close region overlay after finishing
    if let Some(overlay) = app.get_webview_window("recording-overlay") {
        let _ = overlay.close();
    }

    Ok(path)
}

/// Stop scroll capture (keep data for preview)
#[tauri::command]
pub fn stop_scroll_capture(app: AppHandle, state: tauri::State<SharedState>) {
    println!("[DEBUG][shortcut] 停止滚动截图");
    let mut s = state.lock().unwrap();
    s.scroll_capturing = false;

    // Close region overlay if present (matches shortcut-stop behavior)
    if let Some(overlay) = app.get_webview_window("recording-overlay") {
        let _ = overlay.close();
    }
}

/// Cancel scroll capture
#[tauri::command]
pub fn cancel_scroll_capture(app: AppHandle, state: tauri::State<SharedState>) {
    let mut s = state.lock().unwrap();
    s.scroll_capturing = false;
    s.scroll_frames.clear();
    s.scroll_offsets.clear();
    s.scroll_stitched = None;

    // Ensure region overlay is closed when canceling
    if let Some(overlay) = app.get_webview_window("recording-overlay") {
        let _ = overlay.close();
    }
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
            result
                .copy_from(new_frame, 0, base_h)
                .map_err(|e| e.to_string())?;
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
            result
                .copy_from(&cropped, 0, base_h)
                .map_err(|e| e.to_string())?;

            Ok(result)
        }
    } else {
        // Scrolled up: prepend new content at top
        if abs_delta >= new_h {
            // No overlap, just concatenate
            let new_height = new_h + base_h;
            let mut result = RgbaImage::new(base_w, new_height);
            result
                .copy_from(new_frame, 0, 0)
                .map_err(|e| e.to_string())?;
            result
                .copy_from(base, 0, new_h)
                .map_err(|e| e.to_string())?;
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
            result
                .copy_from(&cropped, 0, 0)
                .map_err(|e| e.to_string())?;

            // Copy base image below the new content
            result
                .copy_from(base, 0, pixels_to_add)
                .map_err(|e| e.to_string())?;

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

    // Downscale to max_height for UI preview (trade a bit of CPU for readability)
    let preview = if h > max_height {
        let scale = max_height as f32 / h as f32;
        let new_w = (w as f32 * scale).max(1.0) as u32;
        image::imageops::resize(
            img,
            new_w,
            max_height,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img.clone()
    };

    // Convert RGBA to RGB for JPEG (faster than PNG)
    let rgb_preview = DynamicImage::ImageRgba8(preview).to_rgb8();

    let mut jpg_data = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpg_data, 90);
    encoder
        .encode(
            rgb_preview.as_raw(),
            rgb_preview.width(),
            rgb_preview.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;

    let base64_str = STANDARD.encode(&jpg_data);
    Ok(format!("data:image/jpeg;base64,{}", base64_str))
}

/// Open the scroll overlay window (non-activating panel on macOS)
/// This window won't steal focus, allowing scroll events to pass to underlying windows
#[tauri::command]
pub fn open_scroll_overlay(
    app: AppHandle,
    state: tauri::State<SharedState>,
    region: Region,
) -> Result<(), String> {
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

    // Position the overlay to the right of the selection region
    let panel_width = 320.0;
    let panel_height = 420.0;
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
        s.region = Some(region.clone());
    }

    // Show region indicator overlay (reuse recording overlay window in static mode)
    create_recording_overlay(&app, &region, true);

    // Build window WITHOUT focus - critical for scroll events to pass through
    let win = WebviewWindowBuilder::new(
        &app,
        "scroll-overlay",
        WebviewUrl::App("/scroll-overlay.html".into()),
    )
    .title("Lovshot Scroll")
    .inner_size(panel_width as f64, panel_height as f64)
    .min_inner_size(280.0, 200.0)
    .position(panel_x as f64, panel_y as f64)
    .decorations(false)
    .resizable(true)
    .always_on_top(true)
    .focused(false) // Don't steal focus!
    .transparent(true)
    .build()
    .map_err(|e| e.to_string())?;

    win.show().map_err(|e| e.to_string())?;

    // Set window as non-activating panel on macOS
    #[cfg(target_os = "macos")]
    {
        use objc::{msg_send, sel, sel_impl};
        let _ = win.with_webview(|webview| {
            unsafe {
                let ns_window = webview.ns_window() as *mut objc::runtime::Object;
                // Set high window level so it stays on top
                let _: () = msg_send![ns_window, setLevel: 1000_i64];
                // Get current style mask and add NonactivatingPanel + Resizable
                let current_mask: u64 = msg_send![ns_window, styleMask];
                // NSWindowStyleMaskResizable = 8, NSWindowStyleMaskNonactivatingPanel = 128
                let new_mask = current_mask | 8 | 128;
                let _: () = msg_send![ns_window, setStyleMask: new_mask];
            }
        });
    }

    // Activate the window under the capture region (center point)
    #[cfg(target_os = "macos")]
    {
        let center_x = region.x as f64 + region.width as f64 / 2.0;
        let center_y = region.y as f64 + region.height as f64 / 2.0;
        crate::window_detect::activate_window_at_position(center_x, center_y);
    }

    println!("[DEBUG][open_scroll_overlay] 悬浮窗创建成功 (non-activating)");
    Ok(())
}
