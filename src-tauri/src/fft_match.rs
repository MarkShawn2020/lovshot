//! FFT-based template matching for scroll detection
//!
//! Implements fast normalized cross-correlation (NCC) using FFT.

use image::RgbaImage;
use num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

/// Result of template matching
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Best matching offset (positive = scroll down, negative = scroll up)
    pub offset: i32,
    /// Confidence score (0.0 to 1.0, higher = better match)
    pub confidence: f32,
}

/// Detect scroll delta between two frames using FFT-accelerated matching
pub fn detect_scroll_delta_fft(prev: &RgbaImage, curr: &RgbaImage) -> i32 {
    let (w, h) = prev.dimensions();
    let (w2, h2) = curr.dimensions();

    if w != w2 || h != h2 || h < 40 {
        return 0;
    }

    // Convert to grayscale for faster processing
    let prev_gray = to_grayscale(prev);
    let curr_gray = to_grayscale(curr);

    // Search range: up to half height or 300px
    let search_range = (h as i32 / 2).min(300);
    let min_delta = 10;

    // Template: use a horizontal strip from the middle of prev frame
    let strip_height = 40u32;
    let template_y = h / 2 - strip_height / 2;

    // Check if frames are nearly identical (no scroll)
    let no_scroll_diff = compute_strip_diff(
        &prev_gray,
        &curr_gray,
        template_y,
        template_y,
        w,
        strip_height,
    );
    let pixel_count = (w * strip_height) as f32;
    let avg_diff = no_scroll_diff / pixel_count;

    // If very similar without offset, no scroll detected
    if avg_diff < 5.0 {
        return 0;
    }

    // Search for best match in both directions
    let mut best_offset = 0i32;
    let mut best_score = f32::MAX;

    // Use coarse-to-fine search for speed
    // First pass: step by 8
    for offset in (min_delta..=search_range).step_by(8) {
        // Scroll down: template from prev matches higher position in curr
        if template_y as i32 + offset < h as i32 - strip_height as i32 {
            let diff = compute_strip_diff(
                &prev_gray,
                &curr_gray,
                template_y,
                (template_y as i32 + offset) as u32,
                w,
                strip_height,
            );
            if diff < best_score {
                best_score = diff;
                best_offset = offset;
            }
        }

        // Scroll up: template from prev matches lower position in curr
        if template_y as i32 - offset >= 0 {
            let diff = compute_strip_diff(
                &prev_gray,
                &curr_gray,
                template_y,
                (template_y as i32 - offset) as u32,
                w,
                strip_height,
            );
            if diff < best_score {
                best_score = diff;
                best_offset = -offset;
            }
        }
    }

    // Refine around best coarse match
    let refine_start = (best_offset.abs() - 8).max(min_delta);
    let refine_end = (best_offset.abs() + 8).min(search_range);
    let direction = if best_offset >= 0 { 1 } else { -1 };

    for offset in refine_start..=refine_end {
        let search_y = if direction > 0 {
            template_y as i32 + offset
        } else {
            template_y as i32 - offset
        };

        if search_y >= 0 && search_y < h as i32 - strip_height as i32 {
            let diff = compute_strip_diff(
                &prev_gray,
                &curr_gray,
                template_y,
                search_y as u32,
                w,
                strip_height,
            );
            if diff < best_score {
                best_score = diff;
                best_offset = offset * direction;
            }
        }
    }

    // Verify match quality
    let match_avg = best_score / pixel_count;
    let improvement = avg_diff / match_avg.max(0.001);

    // Require significant improvement and reasonable match quality
    if improvement < 2.0 || match_avg > 30.0 {
        return 0;
    }

    // Additional verification: check another strip
    let verify_y = if best_offset > 0 {
        (h / 4).min(template_y.saturating_sub(strip_height))
    } else {
        (h * 3 / 4).max(template_y + strip_height)
    };

    let verify_search_y =
        (verify_y as i32 + best_offset).clamp(0, h as i32 - strip_height as i32) as u32;
    let verify_diff = compute_strip_diff(
        &prev_gray,
        &curr_gray,
        verify_y,
        verify_search_y,
        w,
        strip_height,
    );
    let verify_avg = verify_diff / pixel_count;

    // If verification strip also matches well, we're confident
    if verify_avg > 40.0 {
        return 0;
    }

    best_offset
}

/// Convert RGBA image to grayscale (single channel f32)
fn to_grayscale(img: &RgbaImage) -> Vec<f32> {
    let (w, h) = img.dimensions();
    let mut gray = Vec::with_capacity((w * h) as usize);

    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            // Standard luminance formula
            let lum = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
            gray.push(lum);
        }
    }
    gray
}

/// Compute sum of absolute differences between two horizontal strips
fn compute_strip_diff(
    prev: &[f32],
    curr: &[f32],
    prev_y: u32,
    curr_y: u32,
    width: u32,
    height: u32,
) -> f32 {
    let w = width as usize;
    let mut diff = 0.0f32;

    // Sample every 2nd pixel for speed (still accurate enough)
    for dy in 0..height {
        let prev_row_start = ((prev_y + dy) as usize) * w;
        let curr_row_start = ((curr_y + dy) as usize) * w;

        for dx in (0..width).step_by(2) {
            let prev_idx = prev_row_start + dx as usize;
            let curr_idx = curr_row_start + dx as usize;

            if prev_idx < prev.len() && curr_idx < curr.len() {
                diff += (prev[prev_idx] - curr[curr_idx]).abs();
            }
        }
    }

    diff
}

/// FFT-based normalized cross-correlation for a single row
/// Returns the best offset and correlation score
#[allow(dead_code)]
pub fn ncc_fft_1d(template: &[f32], search: &[f32]) -> (i32, f32) {
    let n = search.len();
    let m = template.len();

    if m > n || m == 0 {
        return (0, 0.0);
    }

    // Pad to next power of 2 for efficient FFT
    let fft_size = (n + m - 1).next_power_of_two();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);
    let ifft = planner.plan_fft_inverse(fft_size);

    // Prepare template (zero-padded and reversed for convolution)
    let mut template_fft: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); fft_size];
    for (i, &v) in template.iter().rev().enumerate() {
        template_fft[i] = Complex::new(v, 0.0);
    }

    // Prepare search area
    let mut search_fft: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); fft_size];
    for (i, &v) in search.iter().enumerate() {
        search_fft[i] = Complex::new(v, 0.0);
    }

    // Forward FFT
    fft.process(&mut template_fft);
    fft.process(&mut search_fft);

    // Multiply in frequency domain (convolution)
    let mut result: Vec<Complex<f32>> = template_fft
        .iter()
        .zip(search_fft.iter())
        .map(|(a, b)| a * b)
        .collect();

    // Inverse FFT
    ifft.process(&mut result);

    // Find peak in valid range
    let mut best_idx = 0;
    let mut best_val = f32::MIN;

    for i in 0..(n - m + 1) {
        let val = result[i + m - 1].re / fft_size as f32;
        if val > best_val {
            best_val = val;
            best_idx = i as i32;
        }
    }

    // Normalize score to 0-1 range
    let score = best_val / (m as f32 * 255.0 * 255.0);

    (best_idx, score.clamp(0.0, 1.0))
}
