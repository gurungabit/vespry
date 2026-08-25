use crate::controller::PipelineEvent;
use crate::settings::SharedSettings;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// On macOS we don't listen for key events at all: rdev derives modifier
/// press/release from numeric comparisons of raw CGEventFlags, which
/// misclassifies holds of modifier-only hotkeys (right ⌘ reads as a stream
/// of taps). Instead we poll the current modifier-flags state — the same
/// source NSEvent.modifierFlags reads — which needs no event tap and no
/// permissions, with ~33 ms latency.
#[cfg(target_os = "macos")]
mod macos {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceFlagsState(state_id: i32) -> u64;
        fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
    }

    /// kCGEventSourceStateCombinedSessionState
    const COMBINED_SESSION: i32 = 0;

    pub enum Hotkey {
        /// A modifier key, detected via its device-dependent flag bit.
        Flag(u64),
        /// A regular key, detected via its virtual keycode.
        Key(u16),
    }

    pub fn hotkey_from_id(id: &str) -> Hotkey {
        match id {
            "right-alt" => Hotkey::Flag(0x40), // NX_DEVICERALTKEYMASK
            "fn" => Hotkey::Flag(0x0080_0000), // NSEventModifierFlagFunction
            "left-ctrl" => Hotkey::Flag(0x01), // NX_DEVICELCTLKEYMASK
            "f5" => Hotkey::Key(0x60),         // kVK_F5
            _ => Hotkey::Flag(0x10),           // NX_DEVICERCMDKEYMASK (right ⌘)
        }
    }

    pub fn is_down(hotkey: &Hotkey) -> bool {
        unsafe {
            match hotkey {
                Hotkey::Flag(bit) => CGEventSourceFlagsState(COMBINED_SESSION) & bit != 0,
                Hotkey::Key(code) => CGEventSourceKeyState(COMBINED_SESSION, *code),
            }
        }
    }

    pub fn raw_flags() -> u64 {
        unsafe { CGEventSourceFlagsState(COMBINED_SESSION) }
    }
}

#[cfg(target_os = "macos")]
pub fn spawn_listener(tx: Sender<PipelineEvent>, settings: SharedSettings) {
    std::thread::spawn(move || {
        let mut held = false;
        let mut last_flags = macos::raw_flags();
        loop {
            let hotkey = macos::hotkey_from_id(&settings.read().unwrap().hotkey);
            let flags = macos::raw_flags();
            if flags != last_flags {
                log::debug!("modifier flags {last_flags:#x} -> {flags:#x}");
                last_flags = flags;
            }
            let down = macos::is_down(&hotkey);
            if down != held {
                held = down;
                log::info!("hotkey {}", if down { "down" } else { "up" });
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
