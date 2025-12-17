use crate::state::SharedState;
use mouse_position::mouse_position::Mouse;

#[tauri::command]
pub fn get_mouse_position(state: tauri::State<SharedState>) -> Option<(f32, f32)> {
    if let Mouse::Position { x, y } = Mouse::get_mouse_position() {
        let s = state.lock().unwrap();
        let screen_x = s.screen_x;
        let screen_y = s.screen_y;
        let logical_x = x as f32 - screen_x as f32;
        let logical_y = y as f32 - screen_y as f32;
        Some((logical_x, logical_y))
    } else {
        None
    }
}
