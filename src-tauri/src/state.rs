use crate::types::{CaptureMode, Region};
use image::RgbaImage;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub recording: bool,
    pub region: Option<Region>,
    pub frames: Vec<RgbaImage>,
    pub recording_fps: u32,
    pub screen_x: i32,
    pub screen_y: i32,
    pub screen_scale: f32,
    pub pending_mode: Option<CaptureMode>,
    pub screen_snapshot: Option<String>,
    pub shortcuts_paused_for_editing: bool,
    pub shortcuts_paused_for_tray_menu: bool,
    // Scroll capture state
    pub scroll_capturing: bool,
    pub scroll_frames: Vec<RgbaImage>,
    pub scroll_offsets: Vec<i32>, // cumulative scroll offset for each frame
    pub scroll_stitched: Option<RgbaImage>, // the stitched result
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            recording: false,
            region: None,
            frames: Vec::new(),
            recording_fps: 30,
            screen_x: 0,
            screen_y: 0,
            screen_scale: 1.0,
            pending_mode: None,
            screen_snapshot: None,
            shortcuts_paused_for_editing: false,
            shortcuts_paused_for_tray_menu: false,
            scroll_capturing: false,
            scroll_frames: Vec::new(),
            scroll_offsets: Vec::new(),
            scroll_stitched: None,
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
