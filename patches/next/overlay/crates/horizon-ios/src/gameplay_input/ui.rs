//! The UIKit bridge receives a complete, fixed-size snapshot only on a change.
use super::{Controls, Layout, Rect, Settings};

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Button {
    x: f64,
    y: f64,
    side: f64,
    opacity: f64,
    down: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Frame {
    buttons: [Button; super::COUNT],
    knob: [f64; 2],
    scheme: u32,
    mode: u32,
    selected: i32,
    flying: u32,
    values: [f64; 9],
    selection: [f64; 2],
}
impl Frame {
    pub fn new(
        mode: u32,
        layout: &Layout,
        controls: &Controls,
        settings: &Settings,
        flying: bool,
    ) -> Self {
        let selected = controls.selected.filter(|_| mode == 4);
        Self {
            buttons: std::array::from_fn(|i| {
                let Rect { x, y, w, .. } = if i == 0 {
                    controls.floating.unwrap_or(layout.buttons[i])
                } else {
                    layout.buttons[i]
                };
                Button {
                    x,
                    y,
                    side: if mode == 1 || mode == 4 { w } else { 0. },
                    opacity: layout.opacity[i],
                    down: u32::from(controls.down(i)),
                }
            }),
            knob: controls.knob,
            scheme: settings.scheme.number() as u32,
            mode,
            selected: selected.map_or(-1, |i| i as i32),
            flying: u32::from(flying),
            values: settings.values(),
            selection: selected.map_or([1.; 2], |i| {
                let p = settings.layouts[settings.scheme.number()][i];
                [p.size, p.opacity]
            }),
        }
    }
    pub fn send(&self) {
        #[cfg(target_os = "ios")]
        {
            unsafe extern "C" {
                fn horizon_ios_touch_frame(frame: *const Frame);
            }
            // Main thread, synchronous and the borrowed frame is not retained.
            unsafe { horizon_ios_touch_frame(self) };
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_touch_command(command: u32, value: f64) {
    if let Some(proxy) = crate::EVENT_PROXY.get() {
        let _ = proxy.send_event(crate::HostEvent::TouchControls(command, value));
    }
}
