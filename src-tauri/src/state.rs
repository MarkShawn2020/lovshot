use std::sync::{Arc, Mutex};
use image::RgbaImage;
use crate::types::{CaptureMode, Region};

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
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
