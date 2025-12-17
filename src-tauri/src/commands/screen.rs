use crate::capture::Screen;
use base64::{engine::general_purpose::STANDARD, Engine};

#[tauri::command]
pub fn get_screens() -> Vec<serde_json::Value> {
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
pub fn capture_screenshot() -> Result<String, String> {
    let screens = Screen::all().map_err(|e| e.to_string())?;
    if screens.is_empty() {
        return Err("No screens found".to_string());
    }

    let screen = &screens[0];
    let img = screen.capture().map_err(|e| e.to_string())?;

    use image::ImageEncoder;
    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;

    let base64_str = STANDARD.encode(&png_data);
    Ok(format!("data:image/png;base64,{}", base64_str))
}
