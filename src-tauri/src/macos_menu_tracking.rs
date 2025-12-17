use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use objc::runtime::{Class, Object, Sel};
use objc::{class, declare::ClassDecl, msg_send, sel, sel_impl};

use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::shortcuts::register_shortcuts_from_config;
use crate::state::SharedState;

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static STATE: OnceLock<SharedState> = OnceLock::new();
static MENU_TRACKING_COUNT: AtomicUsize = AtomicUsize::new(0);

fn observer_class() -> &'static Class {
    static OBSERVER_CLASS: OnceLock<&'static Class> = OnceLock::new();
    OBSERVER_CLASS.get_or_init(|| {
        let superclass = Class::get("NSObject").expect("NSObject class not found");
        let class_name = "LovshotMenuTrackingObserver";

        if let Some(mut decl) = ClassDecl::new(class_name, superclass) {
            unsafe {
                decl.add_method(
                    sel!(menuDidBeginTracking:),
                    menu_did_begin_tracking as extern "C" fn(&Object, Sel, *mut Object),
                );
                decl.add_method(
                    sel!(menuDidEndTracking:),
                    menu_did_end_tracking as extern "C" fn(&Object, Sel, *mut Object),
                );
            }
            decl.register()
        } else {
            Class::get(class_name).expect("LovshotMenuTrackingObserver class not found")
        }
    })
}

unsafe fn nsstring(s: &str) -> *mut Object {
    let cstr = CString::new(s).expect("CString::new failed");
    msg_send![class!(NSString), stringWithUTF8String: cstr.as_ptr()]
}

pub fn install_menu_tracking_observers(app: &AppHandle, state: SharedState) {
    let _ = APP_HANDLE.set(app.clone());
    let _ = STATE.set(state);

    unsafe {
        let observer: *mut Object = msg_send![observer_class(), new];
        let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];

        let begin_name = nsstring("NSMenuDidBeginTrackingNotification");
        let end_name = nsstring("NSMenuDidEndTrackingNotification");
        let nil: *mut Object = std::ptr::null_mut();

        let _: () = msg_send![
            center,
            addObserver: observer
            selector: sel!(menuDidBeginTracking:)
            name: begin_name
            object: nil
        ];
        let _: () = msg_send![
            center,
            addObserver: observer
            selector: sel!(menuDidEndTracking:)
            name: end_name
            object: nil
        ];
    }
}

extern "C" fn menu_did_begin_tracking(_this: &Object, _cmd: Sel, _notification: *mut Object) {
    let _ = std::panic::catch_unwind(|| {
        let prev = MENU_TRACKING_COUNT.fetch_add(1, Ordering::SeqCst);
        if prev != 0 {
            return;
        }

        let (Some(app), Some(state)) = (APP_HANDLE.get(), STATE.get()) else {
            return;
        };

        let paused_for_editing = {
            let mut s = state.lock().unwrap();
            s.shortcuts_paused_for_tray_menu = true;
            s.shortcuts_paused_for_editing
        };

        if paused_for_editing {
            return;
        }

        let _ = app.global_shortcut().unregister_all();
    });
}

extern "C" fn menu_did_end_tracking(_this: &Object, _cmd: Sel, _notification: *mut Object) {
    let _ = std::panic::catch_unwind(|| {
        let current = MENU_TRACKING_COUNT.load(Ordering::SeqCst);
        if current == 0 {
            return;
        }

        let prev = MENU_TRACKING_COUNT.fetch_sub(1, Ordering::SeqCst);
        if prev != 1 {
            return;
        }

        let (Some(app), Some(state)) = (APP_HANDLE.get(), STATE.get()) else {
            return;
        };

        let paused_for_editing = {
            let mut s = state.lock().unwrap();
            s.shortcuts_paused_for_tray_menu = false;
            s.shortcuts_paused_for_editing
        };

        if paused_for_editing {
            return;
        }

        let _ = register_shortcuts_from_config(app);
    });
}
