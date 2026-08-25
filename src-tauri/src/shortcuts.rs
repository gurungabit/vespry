use crate::controller::PipelineEvent;
use crate::settings::SharedSettings;
use rdev::{EventType, Key};
use std::sync::mpsc::Sender;

/// The hotkey ids offered in settings, mapped to rdev keys.
pub fn key_from_id(id: &str) -> Key {
    match id {
        "right-alt" => Key::AltGr,
        "fn" => Key::Function,
        "left-ctrl" => Key::ControlLeft,
        "f5" => Key::F5,
        _ => Key::MetaRight, // "right-cmd", and the fallback
    }
}

/// Listen for global key events on a dedicated thread and forward
/// press/release of the push-to-talk key to the pipeline. The key is read
/// from settings on every event, so rebinding needs no listener restart.
///
/// On macOS this needs Accessibility permission; without it `rdev::listen`
/// fails and we just log — the app keeps running so the user can grant it.
pub fn spawn_listener(tx: Sender<PipelineEvent>, settings: SharedSettings) {
    std::thread::spawn(move || {
        let mut held = false;
        let result = rdev::listen(move |event| {
            let hotkey = key_from_id(&settings.read().unwrap().hotkey);
            match event.event_type {
                EventType::KeyPress(key) if key == hotkey && !held => {
                    held = true;
                    let _ = tx.send(PipelineEvent::HotkeyPressed);
                }
                EventType::KeyRelease(key) if key == hotkey => {
                    held = false;
                    let _ = tx.send(PipelineEvent::HotkeyReleased);
                }
                _ => {}
            }
        });
        if let Err(e) = result {
            log::error!("global key listener failed (missing Accessibility permission?): {e:?}");
        }
    });
}
