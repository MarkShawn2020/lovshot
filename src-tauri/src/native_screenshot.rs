//! Native macOS screenshot with hardware-accelerated display
//! Uses CGImage → NSImageView pipeline (GPU to GPU, no CPU encoding)

use image::RgbaImage;
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;

// FFI declarations for CoreGraphics
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayCreateImage(display_id: u32) -> *mut c_void;
    fn CGImageGetWidth(image: *const c_void) -> usize;
    fn CGImageGetHeight(image: *const c_void) -> usize;
    fn CGImageGetBytesPerRow(image: *const c_void) -> usize;
    fn CGImageGetDataProvider(image: *const c_void) -> *mut c_void;
    fn CGDataProviderCopyData(provider: *const c_void) -> *mut c_void;
    fn CFDataGetLength(data: *const c_void) -> isize;
    fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
    fn CFRelease(cf: *const c_void);
}

/// Raw CGImage handle
pub struct CGImageRef(*mut c_void);

/// Send-able wrapper for raw CGImage pointer (CGImage is immutable and thread-safe)
#[derive(Clone, Copy)]
pub struct CGImagePtr(pub *mut c_void);
unsafe impl Send for CGImagePtr {}

impl CGImageRef {
    /// Get Send-able pointer for use in 'static closures
    pub fn as_send_ptr(&self) -> CGImagePtr {
        CGImagePtr(self.0)
    }
}

// CGImage is immutable and thread-safe on macOS
unsafe impl Send for CGImageRef {}
unsafe impl Sync for CGImageRef {}

impl Drop for CGImageRef {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

/// Fast screen capture using CoreGraphics (typically 10-50ms)
pub fn capture_cgimage() -> Option<CGImageRef> {
    unsafe {
        let display_id = CGMainDisplayID();
        let cg_image = CGDisplayCreateImage(display_id);
        if cg_image.is_null() {
            None
        } else {
            Some(CGImageRef(cg_image))
        }
    }
}

/// Set window background to CGImage using NSImageView (hardware accelerated)
/// Takes CGImagePtr for use in 'static Send closures
pub unsafe fn set_window_background_cgimage_raw(ns_window: *mut Object, cg_image_ptr: CGImagePtr) {
    // Create NSImage from CGImage
    let ns_image: *mut Object = msg_send![class!(NSImage), alloc];

    #[repr(C)]
    struct NSSize { width: f64, height: f64 }
    let zero_size = NSSize { width: 0.0, height: 0.0 };

    let ns_image: *mut Object = msg_send![ns_image, initWithCGImage:cg_image_ptr.0 size:zero_size];

    // Get content view and its frame
    let content_view: *mut Object = msg_send![ns_window, contentView];

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct NSRect { x: f64, y: f64, width: f64, height: f64 }
    let frame: NSRect = msg_send![content_view, frame];

    // Create NSImageView
    let image_view: *mut Object = msg_send![class!(NSImageView), alloc];
    let image_view: *mut Object = msg_send![image_view, initWithFrame:frame];
    let _: () = msg_send![image_view, setImage:ns_image];
    let _: () = msg_send![image_view, setImageScaling:2_i64]; // NSImageScaleAxesIndependently
    let _: () = msg_send![image_view, setAutoresizingMask:18_u64]; // flexible width + height

    // Insert at the bottom (behind webview)
    let nil: *const Object = std::ptr::null();
    let _: () = msg_send![content_view, addSubview:image_view positioned:-1_i64 relativeTo:nil];
}

/// Convert CGImage to RgbaImage for cropping/saving
pub fn cgimage_to_rgba(cg_image: &CGImageRef) -> Option<RgbaImage> {
    unsafe {
        let width = CGImageGetWidth(cg_image.0) as u32;
        let height = CGImageGetHeight(cg_image.0) as u32;
        let bytes_per_row = CGImageGetBytesPerRow(cg_image.0);

        let provider = CGImageGetDataProvider(cg_image.0);
        if provider.is_null() {
            return None;
        }

        let data = CGDataProviderCopyData(provider);
        if data.is_null() {
            return None;
        }

        let len = CFDataGetLength(data) as usize;
        let ptr = CFDataGetBytePtr(data);
        let bytes = std::slice::from_raw_parts(ptr, len);

        // Convert BGRA to RGBA
        let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            let row_start = y * bytes_per_row;
            for x in 0..width as usize {
                let i = row_start + x * 4;
                if i + 3 < bytes.len() {
                    rgba_data.push(bytes[i + 2]); // R
                    rgba_data.push(bytes[i + 1]); // G
                    rgba_data.push(bytes[i]);     // B
                    rgba_data.push(bytes[i + 3]); // A
                }
            }
        }

        CFRelease(data);
        RgbaImage::from_raw(width, height, rgba_data)
    }
}
