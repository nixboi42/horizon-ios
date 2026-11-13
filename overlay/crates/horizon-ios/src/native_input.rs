//! Main-thread UIKit callbacks enqueue owned data; Game is touched only in its loop.
use horizon_client::game::mobile::Session;
use std::{
    ffi::{CStr, c_char},
    sync::Mutex,
};
use winit::keyboard::KeyCode;

#[derive(Debug)]
enum Input {
    Key(u32),
    Submit(String),
    Edit {
        id: String,
        value: String,
        cursor: usize,
        anchor: usize,
    },
}
static INPUT: Mutex<Vec<Input>> = Mutex::new(Vec::new());

#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_navigation(key: u32) {
    INPUT.lock().unwrap().push(Input::Key(key));
}
/// # Safety
/// `id` must point to a valid NUL-terminated UTF-8 string for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_ios_submit(id: *const c_char) {
    if id.is_null() {
        return;
    }
    if let Ok(id) = unsafe { CStr::from_ptr(id) }.to_str() {
        INPUT.lock().unwrap().push(Input::Submit(id.into()));
    }
}
/// UIKit supplies valid, borrowed, NUL-terminated UTF-8 for this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_ios_edit(
    id: *const c_char,
    value: *const c_char,
    cursor: usize,
    anchor: usize,
) {
    if id.is_null() || value.is_null() {
        return;
    }
    let (Ok(id), Ok(value)) = (
        unsafe { CStr::from_ptr(id) }.to_str(),
        unsafe { CStr::from_ptr(value) }.to_str(),
    ) else {
        return;
    };
    INPUT.lock().unwrap().push(Input::Edit {
        id: id.into(),
        value: value.into(),
        cursor,
        anchor,
    });
}
pub fn sync(game: &mut Session, active: bool, el: &winit::event_loop::ActiveEventLoop) {
    for input in std::mem::take(&mut *INPUT.lock().unwrap()) {
        match input {
            Input::Key(key) => {
                if !active {
                    continue;
                }
                let key = match key {
                    0 => KeyCode::Escape,
                    1 => KeyCode::Enter,
                    2 => KeyCode::KeyT,
                    _ => continue,
                };
                if let Err(error) = game.press(el, key) {
                    log::warn!("UIKit input: {error}");
                }
            }
            Input::Submit(id) => {
                let state = game.input_state();
                if active
                    && state["field"]["id"].as_str() == Some(id.as_str())
                    && let Err(error) = game.press(el, KeyCode::Enter)
                {
                    log::warn!("UIKit submit: {error}");
                }
            }
            Input::Edit {
                id,
                value,
                cursor,
                anchor,
            } => game.edit_text(&id, &value, cursor, anchor),
        }
    }
    let mut state = game.input_state();
    if !active {
        state = serde_json::Value::Null;
    }
    #[cfg(target_os = "ios")]
    {
        unsafe extern "C" {
            fn horizon_ios_input_state(json: *const c_char);
        }
        let json = std::ffi::CString::new(state.to_string()).unwrap();
        unsafe {
            horizon_ios_input_state(json.as_ptr());
        }
    }
    #[cfg(not(target_os = "ios"))]
    let _ = state;
}

/// A physical keyboard event must have exactly one text owner.
pub fn owns_text() -> bool {
    #[cfg(target_os = "ios")]
    {
        unsafe extern "C" {
            fn horizon_ios_text_active() -> u32;
        }
        unsafe { horizon_ios_text_active() != 0 }
    }
    #[cfg(not(target_os = "ios"))]
    {
        false
    }
}
