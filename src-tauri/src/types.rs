use serde::{Deserialize, Serialize};

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

#[derive(Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    pub start_frame: usize,
    pub end_frame: usize,
    pub output_scale: f32,
    pub target_fps: u32,
    pub loop_mode: String, // "infinite", "once", "pingpong"
    #[serde(default = "default_quality")]
    pub quality: u32, // encoding quality (1-100, higher = better quality but slower)
    #[serde(default = "default_speed")]
    pub speed: f32, // playback speed (affects duration, not frame count)
    pub output_path: Option<String>, // custom output path from Finder dialog
}

fn default_quality() -> u32 {
    80
}

fn default_speed() -> f32 {
    1.0
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RecordingInfo {
    pub frame_count: usize,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration_ms: u64,
    pub has_frames: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SizeEstimate {
    pub frame_count: usize,
    pub output_width: u32,
    pub output_height: u32,
    pub estimated_bytes: u64,
    pub formatted: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ExportProgress {
    pub current: usize,
    pub total: usize,
    pub stage: String,
}

#[derive(Clone, Default)]
pub enum GifLoopMode {
    #[default]
    Infinite,
    Once,
    PingPong,
}

/// Capture mode: image (screenshot), gif, video, or scroll (scrolling screenshot)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode {
    #[default]
    Image,
    Gif,
    Video,
    Scroll,
}

/// Progress info for scroll capture preview
#[derive(Clone, Serialize, Deserialize)]
pub struct ScrollCaptureProgress {
    pub frame_count: usize,
    pub total_height: u32,
    pub preview_base64: String,
}

/// Crop edges for scroll capture (percentage from each edge, 0-100)
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct CropEdges {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}
