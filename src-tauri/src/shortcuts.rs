use crate::controller::PipelineEvent;
use rdev::{EventType, Key};
use std::sync::mpsc::Sender;

/// The push-to-talk key. Hardcoded to right ⌘ for now; becomes a setting in M5.
const HOTKEY: Key = Key::MetaRight;

/// Listen for global key events on a dedicated thread and forward
/// press/release of the push-to-talk key to the pipeline.
///
/// On macOS this needs Accessibility permission; without it `rdev::listen`
/// fails and we just log — the app keeps running so the user can grant it.
pub fn spawn_listener(tx: Sender<PipelineEvent>) {
    std::thread::spawn(move || {
        let mut held = false;
        let result = rdev::listen(move |event| match event.event_type {
            EventType::KeyPress(key) if key == HOTKEY && !held => {
                held = true;
                let _ = tx.send(PipelineEvent::HotkeyPressed);
            }
            EventType::KeyRelease(key) if key == HOTKEY => {
                held = false;
                let _ = tx.send(PipelineEvent::HotkeyReleased);
            }
            _ => {}
        });
        if let Err(e) = result {
            log::error!("global key listener failed (missing Accessibility permission?): {e:?}");
        }
    });
}
