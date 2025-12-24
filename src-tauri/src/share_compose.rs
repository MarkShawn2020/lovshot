use ab_glyph::{FontRef, PxScale};
use image::{Rgba, RgbaImage};
use imageproc::drawing::draw_text_mut;
use std::path::PathBuf;

/// Share template types
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareTemplate {
    CaptionBelow,  // 文字在图下（白底）
    Card,          // 卡片式（带圆角边框）
    Minimal,       // 极简（小字号）
    Social,        // 类似即刻/X 风格
}

/// Colors from Lovstudio design system
const BG_WARM: Rgba<u8> = Rgba([249, 249, 247, 255]);      // #F9F9F7
const TEXT_DARK: Rgba<u8> = Rgba([24, 24, 24, 255]);        // #181818
const TEXT_MUTED: Rgba<u8> = Rgba([135, 134, 127, 255]);    // #87867F
const ACCENT: Rgba<u8> = Rgba([204, 120, 92, 255]);         // #CC785C

/// Load system font (PingFang on macOS)
fn load_font() -> Option<FontRef<'static>> {
    #[cfg(target_os = "macos")]
    {
        let font_paths = [
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/Helvetica.ttc",
        ];
        for path in font_paths {
            if let Ok(data) = std::fs::read(path) {
                // Leak to get 'static lifetime (acceptable for font data)
                let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
                if let Ok(font) = FontRef::try_from_slice(leaked) {
                    return Some(font);
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Fallback: try common font paths
        let font_paths = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "C:\\Windows\\Fonts\\msyh.ttc",
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

/// Measure text width (approximate)
fn measure_text_width(text: &str, scale: PxScale) -> u32 {
    // Rough estimate: ~0.5 em per character for CJK, ~0.3 for ASCII
    let mut width = 0.0f32;
    for c in text.chars() {
        if c.is_ascii() {
            width += scale.x * 0.5;
        } else {
            width += scale.x * 1.0;
        }
    }
    width as u32
}

/// Word wrap text to fit within max_width
fn wrap_text(text: &str, scale: PxScale, max_width: u32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0.0f32;

    for c in text.chars() {
        let char_width = if c.is_ascii() { scale.x * 0.5 } else { scale.x * 1.0 };

        if c == '\n' {
            lines.push(current_line.clone());
            current_line.clear();
            current_width = 0.0;
            continue;
        }

        if current_width + char_width > max_width as f32 && !current_line.is_empty() {
            lines.push(current_line.clone());
            current_line.clear();
            current_width = 0.0;
        }

        current_line.push(c);
        current_width += char_width;
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

/// Compose share image with template
pub fn compose_share_image(
    source_path: &str,
    caption: &str,
    template: ShareTemplate,
) -> Result<RgbaImage, String> {
    let font = load_font().ok_or("Failed to load font")?;
    let source = image::open(source_path)
        .map_err(|e| format!("Failed to open image: {}", e))?
        .to_rgba8();

    match template {
        ShareTemplate::CaptionBelow => compose_caption_below(&source, caption, &font),
        ShareTemplate::Card => compose_card(&source, caption, &font),
        ShareTemplate::Minimal => compose_minimal(&source, caption, &font),
        ShareTemplate::Social => compose_social(&source, caption, &font),
    }
}

/// Template: Caption Below - 文字在图下（白底）
fn compose_caption_below(source: &RgbaImage, caption: &str, font: &FontRef) -> Result<RgbaImage, String> {
    let (src_w, src_h) = source.dimensions();
    let padding = 24u32;
    let font_size = 28.0;
    let scale = PxScale::from(font_size);
    let line_height = (font_size * 1.5) as u32;

    // Wrap text
    let max_text_width = src_w.saturating_sub(padding * 2);
    let lines = wrap_text(caption, scale, max_text_width);
    let text_height = (lines.len() as u32) * line_height + padding;

    // Create canvas
    let canvas_h = src_h + text_height + padding;
    let mut canvas = RgbaImage::from_pixel(src_w, canvas_h, BG_WARM);

    // Copy source image
    image::imageops::overlay(&mut canvas, source, 0, 0);

    // Draw text lines
    let text_y_start = src_h + padding / 2;
    for (i, line) in lines.iter().enumerate() {
        let y = text_y_start + (i as u32 * line_height);
        draw_text_mut(&mut canvas, TEXT_DARK, padding as i32, y as i32, scale, font, line);
    }

    Ok(canvas)
}

/// Template: Card - 卡片式（带边框）
fn compose_card(source: &RgbaImage, caption: &str, font: &FontRef) -> Result<RgbaImage, String> {
    let (src_w, src_h) = source.dimensions();
    let card_padding = 20u32;
    let outer_padding = 32u32;
    let font_size = 24.0;
    let scale = PxScale::from(font_size);
    let line_height = (font_size * 1.5) as u32;

    // Wrap text
    let max_text_width = src_w.saturating_sub(card_padding * 2);
    let lines = wrap_text(caption, scale, max_text_width);
    let text_block_height = if lines.is_empty() { 0 } else {
        (lines.len() as u32) * line_height + card_padding
    };

    // Card dimensions
    let card_w = src_w + card_padding * 2;
    let card_h = src_h + card_padding * 2 + text_block_height;

    // Canvas with extra padding around card
    let canvas_w = card_w + outer_padding * 2;
    let canvas_h = card_h + outer_padding * 2;
    let mut canvas = RgbaImage::from_pixel(canvas_w, canvas_h, BG_WARM);

    // Draw card background (white)
    let card_bg = Rgba([255, 255, 255, 255]);
    for y in outer_padding..(outer_padding + card_h) {
        for x in outer_padding..(outer_padding + card_w) {
            canvas.put_pixel(x, y, card_bg);
        }
    }

    // Draw subtle border
    let border_color = Rgba([230, 228, 220, 255]);
    for x in outer_padding..(outer_padding + card_w) {
        canvas.put_pixel(x, outer_padding, border_color);
        canvas.put_pixel(x, outer_padding + card_h - 1, border_color);
    }
    for y in outer_padding..(outer_padding + card_h) {
        canvas.put_pixel(outer_padding, y, border_color);
        canvas.put_pixel(outer_padding + card_w - 1, y, border_color);
    }

    // Copy source image into card
    let img_x = outer_padding + card_padding;
    let img_y = outer_padding + card_padding;
    image::imageops::overlay(&mut canvas, source, img_x as i64, img_y as i64);

    // Draw text
    if !lines.is_empty() {
        let text_y_start = img_y + src_h + card_padding / 2;
        for (i, line) in lines.iter().enumerate() {
            let y = text_y_start + (i as u32 * line_height);
            draw_text_mut(&mut canvas, TEXT_DARK, img_x as i32, y as i32, scale, font, line);
        }
    }

    Ok(canvas)
}

/// Template: Minimal - 极简（小字号）
fn compose_minimal(source: &RgbaImage, caption: &str, font: &FontRef) -> Result<RgbaImage, String> {
    let (src_w, src_h) = source.dimensions();
    let font_size = 16.0;
    let scale = PxScale::from(font_size);
    let padding = 12u32;

    // Single line, truncate if too long
    let max_width = src_w.saturating_sub(padding * 2);
    let mut display_text = caption.replace('\n', " ");
    while measure_text_width(&display_text, scale) > max_width && display_text.len() > 3 {
        display_text.pop();
    }
    if display_text.len() < caption.len() {
        display_text.push_str("…");
    }

    let text_height = (font_size * 1.5) as u32 + padding;
    let canvas_h = src_h + text_height;
    let mut canvas = RgbaImage::from_pixel(src_w, canvas_h, BG_WARM);

    // Copy source
    image::imageops::overlay(&mut canvas, source, 0, 0);

    // Draw text centered
    let text_width = measure_text_width(&display_text, scale);
    let text_x = ((src_w - text_width) / 2) as i32;
    let text_y = (src_h + padding / 2) as i32;
    draw_text_mut(&mut canvas, TEXT_MUTED, text_x, text_y, scale, font, &display_text);

    Ok(canvas)
}

/// Template: Social - 类似即刻/X 风格
fn compose_social(source: &RgbaImage, caption: &str, font: &FontRef) -> Result<RgbaImage, String> {
    let (src_w, src_h) = source.dimensions();
    let padding = 20u32;
    let font_size = 22.0;
    let scale = PxScale::from(font_size);
    let line_height = (font_size * 1.6) as u32;

    // Wrap text
    let content_width = src_w.max(320);
    let max_text_width = content_width.saturating_sub(padding * 2);
    let lines = wrap_text(caption, scale, max_text_width);

    // Text above image
    let text_block_height = if lines.is_empty() { 0 } else {
        (lines.len() as u32) * line_height + padding
    };

    // Watermark height
    let watermark_height = 32u32;

    let canvas_w = content_width;
    let canvas_h = text_block_height + src_h + watermark_height + padding;
    let mut canvas = RgbaImage::from_pixel(canvas_w, canvas_h, Rgba([255, 255, 255, 255]));

    // Draw text at top
    let mut y_offset = padding / 2;
    for line in &lines {
        draw_text_mut(&mut canvas, TEXT_DARK, padding as i32, y_offset as i32, scale, font, line);
        y_offset += line_height;
    }

    // Draw image (centered if narrower than canvas)
    let img_x = ((canvas_w - src_w) / 2) as i64;
    let img_y = text_block_height as i64;
    image::imageops::overlay(&mut canvas, source, img_x, img_y);

    // Draw watermark with optional screenshot number
    let watermark_y = text_block_height + src_h + 8;
    let watermark_scale = PxScale::from(14.0);

    let cfg = crate::config::load_config();
    let watermark = if matches!(cfg.watermark_position, crate::config::WatermarkPosition::Brand) {
        let count = crate::config::count_screenshots();
        format!("#{} via lovshot", count)
    } else {
        "via lovshot".to_string()
    };

    let wm_width = measure_text_width(&watermark, watermark_scale);
    let wm_x = ((canvas_w - wm_width) / 2) as i32;
    draw_text_mut(&mut canvas, TEXT_MUTED, wm_x, watermark_y as i32, watermark_scale, font, &watermark);

    Ok(canvas)
}

/// Tauri command: compose and save share image
#[tauri::command]
pub fn compose_share(
    app: tauri::AppHandle,
    source_path: String,
    caption: String,
    template: String,
) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let template = match template.as_str() {
        "caption_below" => ShareTemplate::CaptionBelow,
        "card" => ShareTemplate::Card,
        "minimal" => ShareTemplate::Minimal,
        "social" => ShareTemplate::Social,
        _ => ShareTemplate::CaptionBelow,
    };

    let composed = compose_share_image(&source_path, &caption, template)?;

    // Copy to clipboard
    let tauri_image = tauri::image::Image::new_owned(
        composed.as_raw().to_vec(),
        composed.width(),
        composed.height(),
    );
    app.clipboard().write_image(&tauri_image)
        .map_err(|e| format!("Clipboard error: {}", e))?;

    // Save to file
    let output_dir = dirs::picture_dir()
        .or_else(|| dirs::home_dir())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lovshot");
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = output_dir.join(format!("share_{}.png", timestamp));
    composed.save(&filename).map_err(|e| format!("Save error: {}", e))?;

    println!("[compose_share] Saved to {:?}", filename);
    Ok(filename.to_string_lossy().to_string())
}
