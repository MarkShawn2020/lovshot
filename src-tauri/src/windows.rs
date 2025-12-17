use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Set macOS activation policy
/// policy: 0 = Regular (normal app, shows in Dock when windows open)
///         1 = Accessory (menu bar app, no Dock icon)
#[cfg(target_os = "macos")]
pub fn set_activation_policy(policy: i64) {
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let ns_app: *mut objc::runtime::Object =
            msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![ns_app, setActivationPolicy: policy];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_activation_policy(_policy: i64) {}

/// Open the settings window
pub fn open_settings_window(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc::{class, msg_send, sel, sel_impl};
        unsafe {
            let ns_app: *mut objc::runtime::Object =
                msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![ns_app, activateIgnoringOtherApps: true];
        }
    }

    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    let win = WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("/settings.html".into()))
        .title("Lovshot Settings")
        .inner_size(480.0, 400.0)
        .resizable(false)
        .center()
        .focused(true)
        .build()
        .map_err(|e| e.to_string())?;

    let _ = win.show();
    let _ = win.set_focus();

    Ok(())
}

/// Open the GIF editor window
pub fn open_editor_window(app: &AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc::{class, msg_send, sel, sel_impl};
        unsafe {
            let ns_app: *mut objc::runtime::Object =
                msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![ns_app, activateIgnoringOtherApps: true];
        }
    }

    // Always create a new editor window (don't reuse)
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let window_label = format!("editor-{}", timestamp);

    let win = WebviewWindowBuilder::new(app, &window_label, WebviewUrl::App("/editor.html".into()))
        .title("Lovshot GIF Editor")
        .inner_size(360.0, 620.0)
        .min_inner_size(320.0, 400.0)
        .resizable(true)
        .center()
        .focused(true)
        .build()
        .map_err(|e| e.to_string())?;

    let _ = win.show();
    let _ = win.set_focus();

    Ok(())
}

/// Open the about window
pub fn open_about_window(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc::{class, msg_send, sel, sel_impl};
        unsafe {
            let ns_app: *mut objc::runtime::Object =
                msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![ns_app, activateIgnoringOtherApps: true];
        }
    }

    if let Some(win) = app.get_webview_window("about") {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    let win = WebviewWindowBuilder::new(&app, "about", WebviewUrl::App("/about.html".into()))
        .title("About Lovshot")
        .inner_size(400.0, 360.0)
        .resizable(false)
        .center()
        .focused(true)
        .build()
        .map_err(|e| e.to_string())?;

    let _ = win.show();
    let _ = win.set_focus();

    Ok(())
}
