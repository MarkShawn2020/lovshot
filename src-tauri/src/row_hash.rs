//! Row-hash based scroll detection
//!
//! Core idea:
//! - Hash each row of pixels (fast FNV-1a)
//! - For scroll DOWN: prev bottom rows should match curr top rows
//! - For scroll UP: prev top rows should match curr bottom rows
//! - Find longest consecutive matching sequence

use image::RgbaImage;

/// Detect scroll delta using row hash matching
/// Returns positive for scroll down, negative for scroll up, 0 for no match
pub fn detect_scroll_delta_row_hash(prev: &RgbaImage, curr: &RgbaImage) -> i32 {
    let (w1, h1) = prev.dimensions();
    let (w2, h2) = curr.dimensions();

    if w1 != w2 || h1 != h2 || h1 < 40 {
        return 0;
    }

    let h = h1 as usize;

    // Hash all rows
    let prev_hashes = hash_all_rows(prev);
    let curr_hashes = hash_all_rows(curr);

    // Identical frames = no scroll
    if prev_hashes == curr_hashes {
        return 0;
    }

    // Config
    let min_overlap = 10; // Minimum rows that must match
    let max_search = h / 2; // Don't search more than half height

    // Try scroll DOWN: prev[h-overlap..h] == curr[0..overlap]
    let down_delta = find_best_overlap(
        &prev_hashes,
        &curr_hashes,
        |overlap| (h - overlap, 0), // prev_start, curr_start
        min_overlap,
        max_search,
    );

    // Try scroll UP: prev[0..overlap] == curr[h-overlap..h]
    let up_delta = find_best_overlap(
        &prev_hashes,
        &curr_hashes,
        |overlap| (0, h - overlap), // prev_start, curr_start
        min_overlap,
        max_search,
    );

    // Return the larger overlap (more confident match)
    match (down_delta, up_delta) {
        (Some(d), Some(u)) => {
            if d >= u {
                d as i32
            } else {
                -(u as i32)
            }
        }
        (Some(d), None) => d as i32,
        (None, Some(u)) => -(u as i32),
        (None, None) => 0,
    }
}

/// Find best overlap using given position function
fn find_best_overlap<F>(
    prev: &[u64],
    curr: &[u64],
    get_positions: F,
    min_overlap: usize,
    max_search: usize,
) -> Option<usize>
where
    F: Fn(usize) -> (usize, usize),
{
    let h = prev.len();

    // Search from large to small overlap (greedy: find biggest match first)
    for overlap in (min_overlap..=max_search).rev() {
        let (prev_start, curr_start) = get_positions(overlap);

        // Check bounds
        if prev_start + overlap > h || curr_start + overlap > h {
            continue;
        }

        // Check if all rows match
        let matches = (0..overlap).all(|i| prev[prev_start + i] == curr[curr_start + i]);

        if matches {
            // Verify with stricter check: ensure non-overlap regions differ
            // (prevents matching on solid color regions)
            let differs = if prev_start > 0 {
                prev[prev_start - 1] != curr[curr_start]
            } else if curr_start > 0 {
                prev[prev_start] != curr[curr_start - 1]
            } else {
                true
            };

            if differs {
                return Some(overlap);
            }
        }
    }

    None
}

/// Hash all rows of an image
fn hash_all_rows(img: &RgbaImage) -> Vec<u64> {
    let (w, h) = img.dimensions();
    (0..h).map(|y| hash_row(img, y, w)).collect()
}

/// FNV-1a hash of a single row
/// Samples every 2nd pixel for speed while maintaining accuracy
#[inline]
fn hash_row(img: &RgbaImage, y: u32, w: u32) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;

    // Sample every 2nd pixel
    for x in (0..w).step_by(2) {
        let p = img.get_pixel(x, y);
        // Combine RGB (skip alpha for consistency)
        hash ^= p[0] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        hash ^= p[1] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        hash ^= p[2] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    hash
}

/// Detect scroll with tolerance for minor pixel differences
/// Uses "fuzzy" row matching by quantizing pixel values
pub fn detect_scroll_delta_fuzzy(prev: &RgbaImage, curr: &RgbaImage) -> i32 {
    let (w1, h1) = prev.dimensions();
    let (w2, h2) = curr.dimensions();

    if w1 != w2 || h1 != h2 || h1 < 40 {
        return 0;
    }

    let h = h1 as usize;
    let min_overlap = 10;
    let max_search = h / 2;

    // Use quantized hashes (tolerant to small color differences)
    let prev_hashes = hash_all_rows_fuzzy(prev);
    let curr_hashes = hash_all_rows_fuzzy(curr);

    if prev_hashes == curr_hashes {
        return 0;
    }

    // Scroll DOWN
    for overlap in (min_overlap..=max_search).rev() {
        let prev_start = h - overlap;
        if (0..overlap).all(|i| prev_hashes[prev_start + i] == curr_hashes[i]) {
            return overlap as i32;
        }
    }

    // Scroll UP
    for overlap in (min_overlap..=max_search).rev() {
        let curr_start = h - overlap;
        if (0..overlap).all(|i| prev_hashes[i] == curr_hashes[curr_start + i]) {
            return -(overlap as i32);
        }
    }

    0
}

/// Hash with quantized values (more tolerant to compression/antialiasing)
fn hash_all_rows_fuzzy(img: &RgbaImage) -> Vec<u64> {
    let (w, h) = img.dimensions();
    (0..h).map(|y| hash_row_fuzzy(img, y, w)).collect()
}

#[inline]
fn hash_row_fuzzy(img: &RgbaImage, y: u32, w: u32) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;

    // Sample every 4th pixel, quantize to 32 levels
    for x in (0..w).step_by(4) {
        let p = img.get_pixel(x, y);
        // Quantize to 5 bits (32 levels) - tolerates ±4 color difference
        let r = (p[0] >> 3) as u64;
        let g = (p[1] >> 3) as u64;
        let b = (p[2] >> 3) as u64;

        hash ^= r | (g << 5) | (b << 10);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_frames() {
        let img = RgbaImage::from_fn(100, 100, |x, y| {
            image::Rgba([x as u8, y as u8, 128, 255])
        });
        assert_eq!(detect_scroll_delta_row_hash(&img, &img), 0);
    }

    #[test]
    fn test_scroll_down() {
        // Create prev frame
        let prev = RgbaImage::from_fn(100, 100, |_x, y| {
            image::Rgba([y as u8, y as u8, y as u8, 255])
        });

        // Create curr frame: shifted up by 20px (scroll down)
        let curr = RgbaImage::from_fn(100, 100, |_x, y| {
            let val = (y + 20).min(119) as u8;
            image::Rgba([val, val, val, 255])
        });

        let delta = detect_scroll_delta_row_hash(&prev, &curr);
        // Should detect ~80 overlap (100 - 20)
        assert!(delta > 0, "Expected positive delta, got {}", delta);
    }
}
