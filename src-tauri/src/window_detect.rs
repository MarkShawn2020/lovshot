use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionaryRef;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::display::{
    kCGNullWindowID, kCGWindowListOptionOnScreenOnly, CGWindowListCopyWindowInfo,
};
use serde::{Deserialize, Serialize};

use crate::Region;

/// Extended window info including titlebar height
#[derive(Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub titlebar_height: u32,
}

/// Get the window bounds under the cursor position
/// Returns None if no window found or on error
pub fn get_window_at_position(x: f64, y: f64) -> Option<Region> {
    unsafe {
        let window_list =
            CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID);

        if window_list.is_null() {
            return None;
        }

        let windows: core_foundation::array::CFArray<CFType> =
            core_foundation::array::CFArray::wrap_under_get_rule(window_list as _);

        // First pass: normal windows only (layer 0)
        // Second pass: Dock (layer 20) - only if no normal window matched
        for target_layer in [0, 20] {
            for i in 0..windows.len() {
                let Some(window) = windows.get(i) else {
                    continue;
                };
                let dict_ref = window.as_CFTypeRef() as CFDictionaryRef;

                // Get window layer
                let layer_key = CFString::new("kCGWindowLayer");
                let layer_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                    dict_ref,
                    layer_key.as_CFTypeRef() as *const _,
                );

                let layer = if !layer_ptr.is_null() {
                    let layer_num: CFNumber = CFNumber::wrap_under_get_rule(layer_ptr as _);
                    layer_num.to_i32().unwrap_or(0)
                } else {
                    0
                };

                if layer != target_layer {
                    continue;
                }

                // Get window bounds
                let bounds_key = CFString::new("kCGWindowBounds");
                let bounds_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                    dict_ref,
                    bounds_key.as_CFTypeRef() as *const _,
                );

                if bounds_ptr.is_null() {
                    continue;
                }

                let bounds_dict = bounds_ptr as CFDictionaryRef;

                let x_key = CFString::new("X");
                let y_key = CFString::new("Y");
                let width_key = CFString::new("Width");
                let height_key = CFString::new("Height");

                let Some(win_x) = get_number_from_dict(bounds_dict, &x_key) else {
                    continue;
                };
                let Some(win_y) = get_number_from_dict(bounds_dict, &y_key) else {
                    continue;
                };
                let Some(win_w) = get_number_from_dict(bounds_dict, &width_key) else {
                    continue;
                };
                let Some(win_h) = get_number_from_dict(bounds_dict, &height_key) else {
                    continue;
                };

                // For Dock (layer 20), use actual visible region from visibleFrame
                if layer == 20 {
                    if let Some(dock_region) = get_dock_region() {
                        // Check if cursor is inside actual Dock bar
                        if x >= dock_region.x as f64
                            && x < (dock_region.x + dock_region.width as i32) as f64
                            && y >= dock_region.y as f64
                            && y < (dock_region.y + dock_region.height as i32) as f64
                        {
                            return Some(dock_region);
                        }
                    }
                    continue;
                }

                // Check if cursor is inside this window
                if x >= win_x && x < win_x + win_w && y >= win_y && y < win_y + win_h {
                    return Some(Region {
                        x: win_x as i32,
                        y: win_y as i32,
                        width: win_w as u32,
                        height: win_h as u32,
                    });
                }
            }
        }

        None
    }
}

/// Get Dock's actual visible region using NSScreen frame vs visibleFrame
fn get_dock_region() -> Option<Region> {
    use core_graphics::geometry::CGRect;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let ns_screen_class = class!(NSScreen);
        let main_screen: *mut objc::runtime::Object = msg_send![ns_screen_class, mainScreen];
        if main_screen.is_null() {
            return None;
        }

        // frame = full screen, visibleFrame = excludes menu bar and dock
        let frame: CGRect = msg_send![main_screen, frame];
        let visible_frame: CGRect = msg_send![main_screen, visibleFrame];

        let screen_height = frame.size.height;
        let screen_width = frame.size.width;

        // Dock height = difference at bottom (visibleFrame.origin.y > 0 means dock at bottom)
        // Note: macOS coordinate system has origin at bottom-left
        let dock_height = visible_frame.origin.y;

        if dock_height > 0.0 {
            // Dock is at bottom - convert to top-left origin coordinate
            Some(Region {
                x: 0,
                y: (screen_height - dock_height) as i32,
                width: screen_width as u32,
                height: dock_height as u32,
            })
        } else {
            // Dock might be on left/right or auto-hidden, check sides
            let left_dock = visible_frame.origin.x;
            let right_dock = screen_width - (visible_frame.origin.x + visible_frame.size.width);

            if left_dock > 0.0 {
                Some(Region {
                    x: 0,
                    y: 0,
                    width: left_dock as u32,
                    height: screen_height as u32,
                })
            } else if right_dock > 0.0 {
                Some(Region {
                    x: (screen_width - right_dock) as i32,
                    y: 0,
                    width: right_dock as u32,
                    height: screen_height as u32,
                })
            } else {
                None // Dock is auto-hidden
            }
        }
    }
}

unsafe fn get_number_from_dict(dict: CFDictionaryRef, key: &CFString) -> Option<f64> {
    let ptr =
        core_foundation::dictionary::CFDictionaryGetValue(dict, key.as_CFTypeRef() as *const _);
    if ptr.is_null() {
        return None;
    }
    let num: CFNumber = CFNumber::wrap_under_get_rule(ptr as _);
    num.to_f64()
}

/// Get the PID of the window at the given position
/// Returns None if no window found
pub fn get_window_pid_at_position(x: f64, y: f64) -> Option<i32> {
    unsafe {
        let window_list =
            CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID);

        if window_list.is_null() {
            return None;
        }

        let windows: core_foundation::array::CFArray<CFType> =
            core_foundation::array::CFArray::wrap_under_get_rule(window_list as _);

        for i in 0..windows.len() {
            let Some(window) = windows.get(i) else {
                continue;
            };
            let dict_ref = window.as_CFTypeRef() as CFDictionaryRef;

            // Get window layer - only consider normal windows (layer 0)
            let layer_key = CFString::new("kCGWindowLayer");
            let layer_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                dict_ref,
                layer_key.as_CFTypeRef() as *const _,
            );

            let layer = if !layer_ptr.is_null() {
                let layer_num: CFNumber = CFNumber::wrap_under_get_rule(layer_ptr as _);
                layer_num.to_i32().unwrap_or(0)
            } else {
                0
            };

            if layer != 0 {
                continue;
            }

            // Get window bounds
            let bounds_key = CFString::new("kCGWindowBounds");
            let bounds_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                dict_ref,
                bounds_key.as_CFTypeRef() as *const _,
            );

            if bounds_ptr.is_null() {
                continue;
            }

            let bounds_dict = bounds_ptr as CFDictionaryRef;

            let x_key = CFString::new("X");
            let y_key = CFString::new("Y");
            let width_key = CFString::new("Width");
            let height_key = CFString::new("Height");

            let Some(win_x) = get_number_from_dict(bounds_dict, &x_key) else {
                continue;
            };
            let Some(win_y) = get_number_from_dict(bounds_dict, &y_key) else {
                continue;
            };
            let Some(win_w) = get_number_from_dict(bounds_dict, &width_key) else {
                continue;
            };
            let Some(win_h) = get_number_from_dict(bounds_dict, &height_key) else {
                continue;
            };

            // Check if cursor is inside this window
            if x >= win_x && x < win_x + win_w && y >= win_y && y < win_y + win_h {
                // Get owning application PID
                let pid_key = CFString::new("kCGWindowOwnerPID");
                let pid_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                    dict_ref,
                    pid_key.as_CFTypeRef() as *const _,
                );

                if pid_ptr.is_null() {
                    return None;
                }

                let pid_num: CFNumber = CFNumber::wrap_under_get_rule(pid_ptr as _);
                return pid_num.to_i32();
            }
        }

        None
    }
}

/// Activate an application by its PID
pub fn activate_app_by_pid(pid: i32) -> bool {
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let workspace_class = class!(NSRunningApplication);
        let running_app: *mut objc::runtime::Object = msg_send![
            workspace_class,
            runningApplicationWithProcessIdentifier: pid
        ];

        if !running_app.is_null() {
            // NSApplicationActivateIgnoringOtherApps = 1 << 1 = 2
            let result: bool = msg_send![running_app, activateWithOptions: 2_u64];
            return result;
        }

        false
    }
}

/// Get application name from PID
fn get_app_name_from_pid(pid: i32) -> Option<String> {
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let workspace_class = class!(NSRunningApplication);
        let running_app: *mut objc::runtime::Object = msg_send![
            workspace_class,
            runningApplicationWithProcessIdentifier: pid
        ];

        if running_app.is_null() {
            return None;
        }

        let name: *mut objc::runtime::Object = msg_send![running_app, localizedName];
        if name.is_null() {
            return None;
        }

        let utf8: *const std::os::raw::c_char = msg_send![name, UTF8String];
        if utf8.is_null() {
            return None;
        }

        Some(
            std::ffi::CStr::from_ptr(utf8)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

/// Get titlebar height based on app name presets + AX fallback
fn get_titlebar_height_for_window(pid: i32, win_bounds: (f64, f64, f64, f64)) -> u32 {
    let app_name = get_app_name_from_pid(pid);
    println!("[titlebar] pid={}, app={:?}", pid, app_name);

    // Preset heights for known apps (titlebar + tabs/toolbar for browsers)
    if let Some(ref name) = app_name {
        let name_lower = name.to_lowercase();

        // Browsers: titlebar + tab bar + address bar
        if name_lower.contains("chrome") || name_lower.contains("chromium") {
            return 87; // Chrome: tabs + address bar
        }
        if name_lower.contains("edge") {
            return 87;
        }
        if name_lower.contains("firefox") {
            return 75;
        }
        if name_lower.contains("safari") {
            return 52; // Safari: tabs + address bar combined
        }
        if name_lower.contains("arc") {
            return 50;
        }

        // Electron apps
        if name_lower.contains("code") || name_lower.contains("vscode") {
            return 87; // VS Code: title + tabs + breadcrumbs
        }
        if name_lower.contains("slack") {
            return 38;
        }
        if name_lower.contains("discord") {
            return 22;
        }
        if name_lower.contains("notion") {
            return 45;
        }

        // Standard macOS apps - try AX detection first
        if name_lower.contains("finder")
            || name_lower.contains("preview")
            || name_lower.contains("notes")
            || name_lower.contains("mail")
            || name_lower.contains("messages")
            || name_lower.contains("terminal")
            || name_lower.contains("iterm")
        {
            // Try AX detection for native apps
            if let Some(h) = try_ax_detection(pid, win_bounds) {
                println!("[titlebar] AX detected height: {}", h);
                return h;
            }
            return 52; // Standard toolbar height
        }
    }

    // Try AX detection for unknown apps
    if let Some(h) = try_ax_detection(pid, win_bounds) {
        println!("[titlebar] AX fallback height: {}", h);
        return h;
    }

    // Default: standard macOS titlebar
    28
}

/// Try to detect titlebar height using Accessibility API (works for native AppKit apps)
fn try_ax_detection(pid: i32, win_bounds: (f64, f64, f64, f64)) -> Option<u32> {
    use accessibility_sys::*;
    use core_foundation::base::TCFType;
    use std::ptr;

    const AX_VALUE_CG_POINT_TYPE: u32 = 1;
    const AX_VALUE_CG_SIZE_TYPE: u32 = 2;

    let (win_x, win_y, win_w, win_h) = win_bounds;

    unsafe {
        let app_element = AXUIElementCreateApplication(pid);
        if app_element.is_null() {
            return None;
        }

        let mut windows_ref: core_foundation::base::CFTypeRef = ptr::null();
        let attr_name = core_foundation::string::CFString::new("AXWindows");
        let result = AXUIElementCopyAttributeValue(
            app_element,
            attr_name.as_concrete_TypeRef(),
            &mut windows_ref,
        );
        core_foundation::base::CFRelease(app_element as _);

        if result != 0 || windows_ref.is_null() {
            return None;
        }

        let windows: core_foundation::array::CFArray<core_foundation::base::CFType> =
            core_foundation::array::CFArray::wrap_under_create_rule(windows_ref as _);

        for i in 0..windows.len() {
            let Some(window) = windows.get(i) else {
                continue;
            };
            let window_ref = window.as_CFTypeRef() as AXUIElementRef;

            // Get window position
            let mut position_ref: core_foundation::base::CFTypeRef = ptr::null();
            let pos_attr = core_foundation::string::CFString::new("AXPosition");
            if AXUIElementCopyAttributeValue(
                window_ref,
                pos_attr.as_concrete_TypeRef(),
                &mut position_ref,
            ) != 0
            {
                continue;
            }
            let mut point = core_graphics::geometry::CGPoint { x: 0.0, y: 0.0 };
            if !AXValueGetValue(
                position_ref as AXValueRef,
                AX_VALUE_CG_POINT_TYPE,
                &mut point as *mut _ as *mut _,
            ) {
                core_foundation::base::CFRelease(position_ref);
                continue;
            }
            core_foundation::base::CFRelease(position_ref);

            // Get window size
            let mut size_ref: core_foundation::base::CFTypeRef = ptr::null();
            let size_attr = core_foundation::string::CFString::new("AXSize");
            if AXUIElementCopyAttributeValue(
                window_ref,
                size_attr.as_concrete_TypeRef(),
                &mut size_ref,
            ) != 0
            {
                continue;
            }
            let mut size = core_graphics::geometry::CGSize {
                width: 0.0,
                height: 0.0,
            };
            if !AXValueGetValue(
                size_ref as AXValueRef,
                AX_VALUE_CG_SIZE_TYPE,
                &mut size as *mut _ as *mut _,
            ) {
                core_foundation::base::CFRelease(size_ref);
                continue;
            }
            core_foundation::base::CFRelease(size_ref);

            // Match window by bounds
            let tolerance = 2.0;
            if (point.x - win_x).abs() > tolerance || (point.y - win_y).abs() > tolerance {
                continue;
            }
            if (size.width - win_w).abs() > tolerance || (size.height - win_h).abs() > tolerance {
                continue;
            }

            // Search for content area
            if let Some(height) = find_content_top_recursive(window_ref, win_y, 0) {
                if height > 0 && height < 150 {
                    return Some(height);
                }
            }
        }

        None
    }
}

/// Recursively search for toolbar or content area to determine titlebar height
unsafe fn find_content_top_recursive(
    element: accessibility_sys::AXUIElementRef,
    win_y: f64,
    depth: u32,
) -> Option<u32> {
    use accessibility_sys::*;
    use core_foundation::base::TCFType;
    use std::ptr;

    const AX_VALUE_CG_POINT_TYPE: u32 = 1;
    const AX_VALUE_CG_SIZE_TYPE: u32 = 2;

    if depth > 3 {
        return None;
    }

    let role_attr = core_foundation::string::CFString::new("AXRole");
    let pos_attr = core_foundation::string::CFString::new("AXPosition");
    let size_attr = core_foundation::string::CFString::new("AXSize");
    let children_attr = core_foundation::string::CFString::new("AXChildren");

    let mut children_ref: core_foundation::base::CFTypeRef = ptr::null();
    if AXUIElementCopyAttributeValue(
        element,
        children_attr.as_concrete_TypeRef(),
        &mut children_ref,
    ) != 0
    {
        return None;
    }

    let children: core_foundation::array::CFArray<core_foundation::base::CFType> =
        core_foundation::array::CFArray::wrap_under_create_rule(children_ref as _);

    let mut best_toolbar_bottom: Option<f64> = None;
    let mut best_content_top: Option<f64> = None;

    for j in 0..children.len() {
        let Some(child) = children.get(j) else {
            continue;
        };
        let child_ref = child.as_CFTypeRef() as AXUIElementRef;

        let mut role_ref: core_foundation::base::CFTypeRef = ptr::null();
        if AXUIElementCopyAttributeValue(child_ref, role_attr.as_concrete_TypeRef(), &mut role_ref)
            != 0
        {
            continue;
        }
        let role_str: core_foundation::string::CFString =
            core_foundation::string::CFString::wrap_under_create_rule(role_ref as _);
        let role = role_str.to_string();

        let mut pos_ref: core_foundation::base::CFTypeRef = ptr::null();
        let mut child_point = core_graphics::geometry::CGPoint { x: 0.0, y: 0.0 };
        if AXUIElementCopyAttributeValue(child_ref, pos_attr.as_concrete_TypeRef(), &mut pos_ref)
            == 0
        {
            AXValueGetValue(
                pos_ref as AXValueRef,
                AX_VALUE_CG_POINT_TYPE,
                &mut child_point as *mut _ as *mut _,
            );
            core_foundation::base::CFRelease(pos_ref);
        }

        let mut size_ref: core_foundation::base::CFTypeRef = ptr::null();
        let mut child_size = core_graphics::geometry::CGSize {
            width: 0.0,
            height: 0.0,
        };
        if AXUIElementCopyAttributeValue(child_ref, size_attr.as_concrete_TypeRef(), &mut size_ref)
            == 0
        {
            AXValueGetValue(
                size_ref as AXValueRef,
                AX_VALUE_CG_SIZE_TYPE,
                &mut child_size as *mut _ as *mut _,
            );
            core_foundation::base::CFRelease(size_ref);
        }

        if role == "AXToolbar" {
            let toolbar_bottom = child_point.y + child_size.height;
            if best_toolbar_bottom.is_none() || toolbar_bottom > best_toolbar_bottom.unwrap() {
                best_toolbar_bottom = Some(toolbar_bottom);
            }
        }

        if role == "AXScrollArea" || role == "AXWebArea" || role == "AXSplitGroup" {
            if best_content_top.is_none() || child_point.y < best_content_top.unwrap() {
                best_content_top = Some(child_point.y);
            }
        }

        if role == "AXGroup" || role == "AXTabGroup" {
            if let Some(h) = find_content_top_recursive(child_ref, win_y, depth + 1) {
                return Some(h);
            }
        }
    }

    if let Some(tb) = best_toolbar_bottom {
        return Some((tb - win_y).max(0.0) as u32);
    }
    if let Some(ct) = best_content_top {
        let h = (ct - win_y).max(0.0) as u32;
        if h > 0 {
            return Some(h);
        }
    }

    None
}

/// Get window info at cursor position including titlebar height
pub fn get_window_info_at_position(x: f64, y: f64) -> Option<WindowInfo> {
    unsafe {
        let window_list =
            CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID);

        if window_list.is_null() {
            return None;
        }

        let windows: core_foundation::array::CFArray<CFType> =
            core_foundation::array::CFArray::wrap_under_get_rule(window_list as _);

        // First pass: normal windows only (layer 0)
        for i in 0..windows.len() {
            let Some(window) = windows.get(i) else {
                continue;
            };
            let dict_ref = window.as_CFTypeRef() as CFDictionaryRef;

            // Get window layer
            let layer_key = CFString::new("kCGWindowLayer");
            let layer_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                dict_ref,
                layer_key.as_CFTypeRef() as *const _,
            );

            let layer = if !layer_ptr.is_null() {
                let layer_num: CFNumber = CFNumber::wrap_under_get_rule(layer_ptr as _);
                layer_num.to_i32().unwrap_or(0)
            } else {
                0
            };

            if layer != 0 {
                continue;
            }

            // Get window bounds
            let bounds_key = CFString::new("kCGWindowBounds");
            let bounds_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                dict_ref,
                bounds_key.as_CFTypeRef() as *const _,
            );

            if bounds_ptr.is_null() {
                continue;
            }

            let bounds_dict = bounds_ptr as CFDictionaryRef;

            let x_key = CFString::new("X");
            let y_key = CFString::new("Y");
            let width_key = CFString::new("Width");
            let height_key = CFString::new("Height");

            let Some(win_x) = get_number_from_dict(bounds_dict, &x_key) else {
                continue;
            };
            let Some(win_y) = get_number_from_dict(bounds_dict, &y_key) else {
                continue;
            };
            let Some(win_w) = get_number_from_dict(bounds_dict, &width_key) else {
                continue;
            };
            let Some(win_h) = get_number_from_dict(bounds_dict, &height_key) else {
                continue;
            };

            // Check if cursor is inside this window
            if x >= win_x && x < win_x + win_w && y >= win_y && y < win_y + win_h {
                // Get PID
                let pid_key = CFString::new("kCGWindowOwnerPID");
                let pid_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                    dict_ref,
                    pid_key.as_CFTypeRef() as *const _,
                );

                let titlebar_height = if !pid_ptr.is_null() {
                    let pid_num: CFNumber = CFNumber::wrap_under_get_rule(pid_ptr as _);
                    if let Some(pid) = pid_num.to_i32() {
                        get_titlebar_height_for_window(pid, (win_x, win_y, win_w, win_h))
                    } else {
                        28
                    }
                } else {
                    28
                };

                return Some(WindowInfo {
                    x: win_x as i32,
                    y: win_y as i32,
                    width: win_w as u32,
                    height: win_h as u32,
                    titlebar_height,
                });
            }
        }

        None
    }
}

/// Activate the app that owns the window under cursor
/// This makes the underlying window receive scroll events
pub fn activate_window_at_position(x: f64, y: f64) -> bool {
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let window_list =
            CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID);

        if window_list.is_null() {
            return false;
        }

        let windows: core_foundation::array::CFArray<CFType> =
            core_foundation::array::CFArray::wrap_under_get_rule(window_list as _);

        for i in 0..windows.len() {
            let Some(window) = windows.get(i) else {
                continue;
            };
            let dict_ref = window.as_CFTypeRef() as CFDictionaryRef;

            // Get window layer - only consider normal windows (layer 0)
            let layer_key = CFString::new("kCGWindowLayer");
            let layer_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                dict_ref,
                layer_key.as_CFTypeRef() as *const _,
            );

            let layer = if !layer_ptr.is_null() {
                let layer_num: CFNumber = CFNumber::wrap_under_get_rule(layer_ptr as _);
                layer_num.to_i32().unwrap_or(0)
            } else {
                0
            };

            if layer != 0 {
                continue;
            }

            // Get window bounds
            let bounds_key = CFString::new("kCGWindowBounds");
            let bounds_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                dict_ref,
                bounds_key.as_CFTypeRef() as *const _,
            );

            if bounds_ptr.is_null() {
                continue;
            }

            let bounds_dict = bounds_ptr as CFDictionaryRef;

            let x_key = CFString::new("X");
            let y_key = CFString::new("Y");
            let width_key = CFString::new("Width");
            let height_key = CFString::new("Height");

            let Some(win_x) = get_number_from_dict(bounds_dict, &x_key) else {
                continue;
            };
            let Some(win_y) = get_number_from_dict(bounds_dict, &y_key) else {
                continue;
            };
            let Some(win_w) = get_number_from_dict(bounds_dict, &width_key) else {
                continue;
            };
            let Some(win_h) = get_number_from_dict(bounds_dict, &height_key) else {
                continue;
            };

            // Check if cursor is inside this window
            if x >= win_x && x < win_x + win_w && y >= win_y && y < win_y + win_h {
                // Get owning application PID
                let pid_key = CFString::new("kCGWindowOwnerPID");
                let pid_ptr = core_foundation::dictionary::CFDictionaryGetValue(
                    dict_ref,
                    pid_key.as_CFTypeRef() as *const _,
                );

                if pid_ptr.is_null() {
                    return false;
                }

                let pid_num: CFNumber = CFNumber::wrap_under_get_rule(pid_ptr as _);
                let Some(pid) = pid_num.to_i32() else {
                    return false;
                };

                // Activate the application using NSRunningApplication
                let workspace_class = class!(NSRunningApplication);
                let running_app: *mut objc::runtime::Object = msg_send![
                    workspace_class,
                    runningApplicationWithProcessIdentifier: pid
                ];

                if !running_app.is_null() {
                    // NSApplicationActivateIgnoringOtherApps = 1 << 1 = 2
                    let _: bool = msg_send![running_app, activateWithOptions: 2_u64];
                    return true;
                }

                return false;
            }
        }

        false
    }
}
