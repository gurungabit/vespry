use crate::controller::PipelineEvent;
use crate::settings::SharedSettings;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// On macOS we don't listen for key events at all: rdev derives modifier
/// press/release from numeric comparisons of raw CGEventFlags, which
/// misclassifies holds of modifier-only hotkeys (right ⌘ reads as a stream
/// of taps). Instead we poll the physical key state via
/// CGEventSourceKeyState — ground truth, ~33 ms latency, no event tap and
/// no Accessibility needed for the hotkey itself.
#[cfg(target_os = "macos")]
mod macos {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
    }

    /// kCGEventSourceStateHIDSystemState
    const HID_STATE: i32 = 1;

    /// Virtual keycodes for the hotkey ids offered in settings.
    pub fn keycode_from_id(id: &str) -> u16 {
        match id {
            "right-alt" => 0x3D,  // right option
            "fn" => 0x3F,         // fn/globe
            "left-ctrl" => 0x3B,  // left control
            "f5" => 0x60,         // F5
            _ => 0x36,            // right command ("right-cmd" and the fallback)
        }
    }

    pub fn key_is_down(keycode: u16) -> bool {
        unsafe { CGEventSourceKeyState(HID_STATE, keycode) }
    }
}

#[cfg(target_os = "macos")]
pub fn spawn_listener(tx: Sender<PipelineEvent>, settings: SharedSettings) {
    std::thread::spawn(move || {
        let mut held = false;
        loop {
            let keycode = macos::keycode_from_id(&settings.read().unwrap().hotkey);
            let down = macos::key_is_down(keycode);
            if down != held {
                held = down;
                log::debug!("hotkey {}", if down { "down" } else { "up" });
                let event = if down {
                    PipelineEvent::HotkeyPressed
                } else {
                    PipelineEvent::HotkeyReleased
                };
                if tx.send(event).is_err() {
                    return; // pipeline gone; stop polling
                }
            }
            std::thread::sleep(Duration::from_millis(33));
        }
    });
}

/// The hotkey ids offered in settings, mapped to rdev keys (non-macOS path).
#[cfg(not(target_os = "macos"))]
fn key_from_id(id: &str) -> rdev::Key {
    match id {
        "right-alt" => rdev::Key::AltGr,
        "fn" => rdev::Key::Function,
        "left-ctrl" => rdev::Key::ControlLeft,
        "f5" => rdev::Key::F5,
        _ => rdev::Key::MetaRight, // "right-cmd", and the fallback
    }
}

/// Global key listener via rdev (Windows/Linux, once those ports land).
#[cfg(not(target_os = "macos"))]
pub fn spawn_listener(tx: Sender<PipelineEvent>, settings: SharedSettings) {
    std::thread::spawn(move || {
        let mut held = false;
        let result = rdev::listen(move |event| {
            let hotkey = key_from_id(&settings.read().unwrap().hotkey);
            match event.event_type {
                rdev::EventType::KeyPress(key) if key == hotkey && !held => {
                    held = true;
                    let _ = tx.send(PipelineEvent::HotkeyPressed);
                }
                rdev::EventType::KeyRelease(key) if key == hotkey => {
                    held = false;
                    let _ = tx.send(PipelineEvent::HotkeyReleased);
                }
                _ => {}
            }
        });
        if let Err(e) = result {
            log::error!("global key listener failed: {e:?}");
        }
    });
}
