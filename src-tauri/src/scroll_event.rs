//! macOS scroll event listener using CGEventTap
//!
//! Listens for global scroll wheel events and triggers capture when scrolling occurs.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType, EventField,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::SharedState;
use crate::types::ScrollCaptureProgress;

/// Global flag to control the event tap
static SCROLL_LISTENER_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Count consecutive "no match" results to avoid infinite retry
static NO_MATCH_COUNT: AtomicU64 = AtomicU64::new(0);

/// Result of scroll capture attempt
enum CaptureResult {
    /// Successfully captured and stitched
    Success(ScrollCaptureProgress),
    /// Frames are identical - content hasn't scrolled yet
    FramesIdentical,
    /// No match found in search range
    NoMatch,
    /// Not in capture mode or other error
    Error,
}

/// Perform a single scroll capture iteration
fn do_scroll_capture(
    state: &SharedState,
    expected_direction: i32,
    _delta_y: f64,
    _use_fixed_delta: bool,
) -> CaptureResult {
    use crate::capture::Screen;
    use crate::commands::{generate_preview_base64, stitch_scroll_image};
    use crate::fft_match::detect_scroll_delta_fft;
    use image::RgbaImage;

    // Get required data with minimal lock time
    let (region, last_frame, scroll_stitched) = {
        let s = match state.lock() {
            Ok(s) => s,
            Err(_) => return CaptureResult::Error,
        };
        if !s.scroll_capturing {
            return CaptureResult::Error;
        }
        match (s.region.clone(), s.scroll_frames.last().cloned(), s.scroll_stitched.clone()) {
            (Some(r), Some(f), Some(st)) => (r, f, st),
            _ => return CaptureResult::Error,
        }
    };

    // Capture new frame
    let screens = match Screen::all() {
        Ok(s) => s,
        Err(_) => return CaptureResult::Error,
    };
    let screen = match screens.first() {
        Some(s) => s,
        None => return CaptureResult::Error,
    };
    let captured = match screen.capture_area(region.x, region.y, region.width, region.height) {
        Ok(c) => c,
        Err(_) => return CaptureResult::Error,
    };

    let new_frame = match RgbaImage::from_raw(captured.width(), captured.height(), captured.into_raw()) {
        Some(f) => f,
        None => return CaptureResult::Error,
    };

    // Use larger search range - don't limit based on delta estimate
    // Real scroll can be much larger than event delta suggests
    let max_delta = 300; // Search up to 300px

    let scroll_delta =
        detect_scroll_delta_fft(&last_frame, &new_frame, expected_direction, Some(max_delta));

    if scroll_delta == 0 {
        // Check if frames are nearly identical (content hasn't moved yet)
        let identical = frames_nearly_identical(&last_frame, &new_frame);
        if identical {
            return CaptureResult::FramesIdentical;
        }
        return CaptureResult::NoMatch;
    }

    println!("[scroll_event] match delta {}", scroll_delta);

    // Stitch the image
    let stitched = match stitch_scroll_image(&scroll_stitched, &new_frame, scroll_delta) {
        Ok(s) => s,
        Err(_) => return CaptureResult::Error,
    };

    // Calculate new offset
    let last_offset = {
        let s = match state.lock() {
            Ok(s) => s,
            Err(_) => return CaptureResult::Error,
        };
        *s.scroll_offsets.last().unwrap_or(&0)
    };
    let new_offset = last_offset + scroll_delta;

    // Generate preview
    let preview = match generate_preview_base64(&stitched, 600) {
        Ok(p) => p,
        Err(_) => return CaptureResult::Error,
    };

    // Update state
    let mut s = match state.lock() {
        Ok(s) => s,
        Err(_) => return CaptureResult::Error,
    };
    if !s.scroll_capturing {
        return CaptureResult::Error;
    }

    s.scroll_frames.push(new_frame);
    s.scroll_offsets.push(new_offset);
    s.scroll_stitched = Some(stitched);

    let frame_count = s.scroll_frames.len();
    let total_height = match s.scroll_stitched.as_ref() {
        Some(img) => img.height(),
        None => return CaptureResult::Error,
    };

    CaptureResult::Success(ScrollCaptureProgress {
        frame_count,
        total_height,
        preview_base64: preview,
    })
}

/// Check if two frames are nearly identical (no visible change)
fn frames_nearly_identical(a: &image::RgbaImage, b: &image::RgbaImage) -> bool {
    let (w, h) = a.dimensions();
    if b.dimensions() != (w, h) {
        return false;
    }

    // Sample a few rows from middle of image
    let sample_rows = [h / 4, h / 2, h * 3 / 4];
    let mut total_diff = 0u64;
    let mut samples = 0u64;

    for &y in &sample_rows {
        for x in (0..w).step_by(4) {
            let pa = a.get_pixel(x, y);
            let pb = b.get_pixel(x, y);
            let diff = (pa[0] as i32 - pb[0] as i32).unsigned_abs()
                + (pa[1] as i32 - pb[1] as i32).unsigned_abs()
                + (pa[2] as i32 - pb[2] as i32).unsigned_abs();
            total_diff += diff as u64;
            samples += 1;
        }
    }

    let avg_diff = total_diff as f64 / samples.max(1) as f64;
    avg_diff < 5.0 // Very similar = content hasn't scrolled
}

/// Start listening for global scroll events
pub fn start_scroll_listener(app: AppHandle) {
    if SCROLL_LISTENER_ACTIVE.swap(true, Ordering::SeqCst) {
        println!("[scroll_event] Listener already active");
        return;
    }

    // Reset no-match counter
    NO_MATCH_COUNT.store(0, Ordering::Relaxed);

    thread::spawn(move || {
        println!("[scroll_event] Starting global scroll listener");

        // Debounce state
        let last_capture = Arc::new(std::sync::Mutex::new(
            Instant::now() - Duration::from_millis(300),
        ));
        let last_capture_clone = last_capture.clone();
        let app_clone = app.clone();
        let scroll_accum = Arc::new(std::sync::Mutex::new(0.0f64));
        let scroll_dir = Arc::new(std::sync::Mutex::new(0i32));
        let scroll_accum_clone = scroll_accum.clone();
        let scroll_dir_clone = scroll_dir.clone();

        // Create event tap for scroll wheel events
        let tap = CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![CGEventType::ScrollWheel],
            move |_proxy, _event_type, event| {
                if !SCROLL_LISTENER_ACTIVE.load(Ordering::Relaxed) {
                    return None;
                }

                // Skip if too many consecutive failures
                if NO_MATCH_COUNT.load(Ordering::Relaxed) > 10 {
                    return None;
                }

                // Get scroll delta
                let point_delta = event
                    .get_double_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1);
                let fixed_delta = event
                    .get_double_value_field(EventField::SCROLL_WHEEL_EVENT_FIXED_POINT_DELTA_AXIS_1);
                let is_continuous = event
                    .get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_IS_CONTINUOUS);

                let (delta_y, use_fixed_delta) = if fixed_delta.abs() > 0.1 {
                    (fixed_delta, true)
                } else {
                    (point_delta, false)
                };
                let delta_sign = if delta_y < 0.0 { -1 } else { 1 };

                // Higher threshold for continuous (trackpad) scrolling
                let threshold = if is_continuous != 0 { 8.0 } else { 1.0 };

                if delta_y.abs() > 0.1 {
                    let mut accum = scroll_accum_clone.lock().unwrap();
                    let mut dir = scroll_dir_clone.lock().unwrap();

                    // Direction change resets accumulator
                    if *dir != 0 && *dir != delta_sign {
                        *accum = 0.0;
                    }
                    *dir = delta_sign;
                    *accum += delta_y;
                    let accum_snapshot = *accum;

                    // Not enough accumulated scroll yet
                    if accum_snapshot.abs() < threshold {
                        return None;
                    }

                    let mut last = last_capture_clone.lock().unwrap();
                    let now = Instant::now();

                    // Dynamic debounce: longer wait after failures
                    let no_match = NO_MATCH_COUNT.load(Ordering::Relaxed);
                    let debounce_ms = if no_match > 0 {
                        150 + (no_match as u64 * 50).min(200) // 150-350ms after failures
                    } else {
                        120 // Normal: 120ms
                    };

                    if now.duration_since(*last) < Duration::from_millis(debounce_ms) {
                        return None;
                    }

                    *last = now;
                    *accum = 0.0; // Reset accumulator when attempting capture
                    drop(accum);
                    drop(dir);
                    drop(last);

                    if let Some(state) = app_clone.try_state::<SharedState>() {
                        let expected_direction = if delta_y < 0.0 { 1 } else { -1 };
                        match do_scroll_capture(&state, expected_direction, accum_snapshot, use_fixed_delta) {
                            CaptureResult::Success(progress) => {
                                NO_MATCH_COUNT.store(0, Ordering::Relaxed);
                                let _ = app_clone.emit("scroll-preview-update", &progress);
                                println!(
                                    "[scroll_event] frame {} height {}",
                                    progress.frame_count, progress.total_height
                                );
                            }
                            CaptureResult::FramesIdentical => {
                                // Content hasn't moved yet - wait longer
                                NO_MATCH_COUNT.fetch_add(1, Ordering::Relaxed);
                            }
                            CaptureResult::NoMatch => {
                                // Couldn't match - maybe dynamic content
                                NO_MATCH_COUNT.fetch_add(1, Ordering::Relaxed);
                                println!("[scroll_event] no match");
                            }
                            CaptureResult::Error => {
                                // Capture mode ended or other error
                            }
                        }
                    }
                }

                None
            },
        );

        match tap {
            Ok(tap) => {
                let source = tap
                    .mach_port
                    .create_runloop_source(0)
                    .expect("Failed to create run loop source");

                unsafe {
                    let run_loop = CFRunLoop::get_current();
                    run_loop.add_source(&source, kCFRunLoopDefaultMode);
                    tap.enable();
                    let _ = app.emit("scroll-listener-started", ());

                    println!("[scroll_event] Scroll listener started successfully");

                    while SCROLL_LISTENER_ACTIVE.load(Ordering::Relaxed) {
                        CFRunLoop::run_in_mode(
                            kCFRunLoopDefaultMode,
                            Duration::from_millis(100),
                            false,
                        );
                    }

                    run_loop.remove_source(&source, kCFRunLoopDefaultMode);
                }

                println!("[scroll_event] Scroll listener stopped");
            }
            Err(e) => {
                eprintln!("[scroll_event] Failed to create event tap: {:?}", e);
                eprintln!("[scroll_event] This requires Accessibility permission");
                SCROLL_LISTENER_ACTIVE.store(false, Ordering::Relaxed);
                let _ = app.emit("scroll-listener-failed", ());
            }
        }
    });
}

/// Stop the global scroll listener
pub fn stop_scroll_listener() {
    println!("[scroll_event] Stopping scroll listener");
    SCROLL_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
}
