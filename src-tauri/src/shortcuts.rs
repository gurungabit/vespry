use crate::controller::PipelineEvent;
use crate::settings::SharedSettings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

/// Set while the settings window is recording a new shortcut, so the keys
/// being pressed there don't also start a dictation.
pub type CaptureFlag = Arc<AtomicBool>;

/// A shortcut is stored as a `+`-joined spec: zero or more modifier tokens
/// (`cmd`, `right-cmd`, `left-ctrl`, `shift`, `fn`, …) followed by an
/// optional physical key named by its DOM `code` (`KeyD`, `F5`, `Space`).
/// The old fixed ids — `right-cmd`, `right-alt`, `fn`, `left-ctrl`, `f5` —
/// are all valid specs, so saved settings keep working.
///
/// Modifier-only specs are what push-to-talk really wants: we poll key state
/// rather than swallow events, so a spec with a regular key still types that
/// key into whatever app is focused.
pub const DEFAULT: &str = "right-cmd";

/// Modifier aliases → the canonical token. Anything not listed here is
/// treated as the shortcut's key.
fn canonical_modifier(token: &str) -> Option<&'static str> {
    const NAMES: &[(&str, &str)] = &[
        ("fn", "fn"),
        ("globe", "fn"),
        ("ctrl", "ctrl"),
        ("control", "ctrl"),
        ("left-ctrl", "left-ctrl"),
        ("right-ctrl", "right-ctrl"),
        ("alt", "alt"),
        ("option", "alt"),
        ("left-alt", "left-alt"),
        ("right-alt", "right-alt"),
        ("shift", "shift"),
        ("left-shift", "left-shift"),
        ("right-shift", "right-shift"),
        ("cmd", "cmd"),
        ("command", "cmd"),
        ("meta", "cmd"),
        ("left-cmd", "left-cmd"),
        ("right-cmd", "right-cmd"),
    ];
    NAMES
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(token))
        .map(|(_, name)| *name)
}

struct Spec<'a> {
    mods: Vec<&'static str>,
    key: Option<&'a str>,
}

fn parse_spec(spec: &str) -> Spec<'_> {
    let mut mods = Vec::new();
    let mut key = None;
    for token in spec.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        match canonical_modifier(token) {
            Some(name) => {
                if !mods.contains(&name) {
                    mods.push(name);
                }
            }
            // A spec has at most one key; a later one wins.
            None => key = Some(token),
        }
    }
    Spec { mods, key }
}

/// On macOS we don't listen for key events at all: rdev derives modifier
/// press/release from numeric comparisons of raw CGEventFlags, which
/// misclassifies holds of modifier-only hotkeys (right ⌘ reads as a stream
/// of taps). Instead we poll the current modifier-flags state — the same
/// source NSEvent.modifierFlags reads — which needs no event tap and no
/// permissions, with ~33 ms latency.
#[cfg(target_os = "macos")]
mod macos {
    use super::{parse_spec, DEFAULT};

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceFlagsState(state_id: i32) -> u64;
        fn CGEventSourceKeyState(state_id: i32, key: u16) -> bool;
    }

    /// kCGEventSourceStateCombinedSessionState
    const COMBINED_SESSION: i32 = 0;

    // Device-independent flags: which modifier is held, either side.
    const SHIFT: u64 = 0x0002_0000;
    const CTRL: u64 = 0x0004_0000;
    const ALT: u64 = 0x0008_0000;
    const CMD: u64 = 0x0010_0000;
    const FN: u64 = 0x0080_0000;
    const ALL_GROUPS: u64 = SHIFT | CTRL | ALT | CMD | FN;

    /// `(token, device-dependent side bit, device-independent group)`. The
    /// side bit is 0 for the "either side" tokens; the NX_DEVICE* bits are
    /// what let us tell right ⌘ from left ⌘.
    const MODIFIERS: &[(&str, u64, u64)] = &[
        ("fn", 0, FN),
        ("ctrl", 0, CTRL),
        ("left-ctrl", 0x01, CTRL),
        ("right-ctrl", 0x2000, CTRL),
        ("alt", 0, ALT),
        ("left-alt", 0x20, ALT),
        ("right-alt", 0x40, ALT),
        ("shift", 0, SHIFT),
        ("left-shift", 0x02, SHIFT),
        ("right-shift", 0x04, SHIFT),
        ("cmd", 0, CMD),
        ("left-cmd", 0x08, CMD),
        ("right-cmd", 0x10, CMD),
    ];

    /// DOM `code` → macOS virtual keycode, for the keys worth binding.
    #[rustfmt::skip]
    const KEYS: &[(&str, u16)] = &[
        ("KeyA", 0x00), ("KeyB", 0x0B), ("KeyC", 0x08), ("KeyD", 0x02),
        ("KeyE", 0x0E), ("KeyF", 0x03), ("KeyG", 0x05), ("KeyH", 0x04),
        ("KeyI", 0x22), ("KeyJ", 0x26), ("KeyK", 0x28), ("KeyL", 0x25),
        ("KeyM", 0x2E), ("KeyN", 0x2D), ("KeyO", 0x1F), ("KeyP", 0x23),
        ("KeyQ", 0x0C), ("KeyR", 0x0F), ("KeyS", 0x01), ("KeyT", 0x11),
        ("KeyU", 0x20), ("KeyV", 0x09), ("KeyW", 0x0D), ("KeyX", 0x07),
        ("KeyY", 0x10), ("KeyZ", 0x06), ("Digit0", 0x1D), ("Digit1", 0x12),
        ("Digit2", 0x13), ("Digit3", 0x14), ("Digit4", 0x15), ("Digit5", 0x17),
        ("Digit6", 0x16), ("Digit7", 0x1A), ("Digit8", 0x1C), ("Digit9", 0x19),
        ("F1", 0x7A), ("F2", 0x78), ("F3", 0x63), ("F4", 0x76), ("F5", 0x60),
        ("F6", 0x61), ("F7", 0x62), ("F8", 0x64), ("F9", 0x65), ("F10", 0x6D),
        ("F11", 0x67), ("F12", 0x6F), ("F13", 0x69), ("F14", 0x6B), ("F15", 0x71),
        ("F16", 0x6A), ("F17", 0x40), ("F18", 0x4F), ("F19", 0x50), ("F20", 0x5A),
        ("Space", 0x31), ("Enter", 0x24), ("Tab", 0x30), ("Escape", 0x35),
        ("Backspace", 0x33), ("Delete", 0x75), ("Home", 0x73), ("End", 0x77),
        ("PageUp", 0x74), ("PageDown", 0x79), ("ArrowLeft", 0x7B), ("ArrowRight", 0x7C),
        ("ArrowDown", 0x7D), ("ArrowUp", 0x7E), ("Minus", 0x1B), ("Equal", 0x18),
        ("BracketLeft", 0x21), ("BracketRight", 0x1E), ("Backslash", 0x2A),
        ("Semicolon", 0x29), ("Quote", 0x27), ("Comma", 0x2B), ("Period", 0x2F),
        ("Slash", 0x2C), ("Backquote", 0x32),
    ];

    fn keycode(name: &str) -> Option<u16> {
        KEYS.iter()
            .find(|(code, _)| code.eq_ignore_ascii_case(name))
            .map(|(_, vk)| *vk)
    }

    #[derive(Debug, PartialEq)]
    pub struct Hotkey {
        /// Device-independent bits that must be set…
        groups: u64,
        /// …exactly, across this mask. Fn is only checked when the shortcut
        /// asks for it: laptops set it for F-keys and arrows too.
        mask: u64,
        /// Device-dependent bits pinning a modifier to one side.
        sides: u64,
        /// Virtual keycode that must also be down, for non-modifier keys.
        key: Option<u16>,
    }

    pub fn parse(spec: &str) -> Hotkey {
        let parsed = parse_spec(spec);
        let key = parsed.key.and_then(keycode);
        let mut hotkey = Hotkey {
            groups: 0,
            mask: ALL_GROUPS & !FN,
            sides: 0,
            key,
        };
        for name in &parsed.mods {
            let Some((_, side, group)) = MODIFIERS.iter().find(|(n, ..)| n == name) else {
                continue;
            };
            hotkey.groups |= group;
            hotkey.sides |= side;
            if *group == FN {
                hotkey.mask |= FN;
            }
        }
        // An unparseable spec (a key we don't know, or nothing at all) would
        // otherwise mean "fires whenever no modifier is held".
        if hotkey.groups == 0 && hotkey.key.is_none() {
            if spec == DEFAULT {
                // Guard against a typo in DEFAULT recursing forever.
                return Hotkey {
                    groups: CMD,
                    mask: ALL_GROUPS & !FN,
                    sides: 0x10,
                    key: None,
                };
            }
            log::warn!("unrecognized shortcut {spec:?}; falling back to {DEFAULT}");
            return parse(DEFAULT);
        }
        hotkey
    }

    pub fn is_down(hotkey: &Hotkey) -> bool {
        let flags = raw_flags();
        if flags & hotkey.mask != hotkey.groups {
            return false;
        }
        if flags & hotkey.sides != hotkey.sides {
            return false;
        }
        match hotkey.key {
            Some(code) => unsafe { CGEventSourceKeyState(COMBINED_SESSION, code) },
            None => true,
        }
    }

    pub fn raw_flags() -> u64 {
        unsafe { CGEventSourceFlagsState(COMBINED_SESSION) }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn legacy_ids_still_parse() {
            let right_cmd = parse("right-cmd");
            assert_eq!(right_cmd.groups, CMD);
            assert_eq!(right_cmd.sides, 0x10);
            assert_eq!(right_cmd.key, None);

            let f5 = parse("f5");
            assert_eq!(f5.groups, 0);
            assert_eq!(f5.key, Some(0x60));

            // Fn is only part of the exact-match mask when asked for.
            assert_eq!(parse("fn").groups, FN);
            assert_eq!(parse("fn").mask & FN, FN);
            assert_eq!(parse("f5").mask & FN, 0);
        }

        #[test]
        fn combinations_and_aliases() {
            let combo = parse("ctrl+shift+KeyD");
            assert_eq!(combo.groups, CTRL | SHIFT);
            assert_eq!(combo.sides, 0);
            assert_eq!(combo.key, Some(0x02));

            assert_eq!(parse("Control+Option+Space"), parse("ctrl+alt+Space"));
            assert_eq!(parse("command"), parse("cmd"));
        }

        /// The settings window builds specs from DOM key codes; keep the two
        /// sides speaking the same language.
        #[test]
        fn specs_the_recorder_produces() {
            let combo = parse("left-ctrl+left-shift+KeyD");
            assert_eq!(combo.groups, CTRL | SHIFT);
            assert_eq!(combo.sides, 0x01 | 0x02);
            assert_eq!(combo.key, Some(0x02));

            let space = parse("right-alt+Space");
            assert_eq!(space.groups, ALT);
            assert_eq!(space.sides, 0x40);
            assert_eq!(space.key, Some(0x31));

            let globe = parse("fn+KeyV");
            assert_eq!(globe.groups, FN);
            assert_eq!(globe.key, Some(0x09));

            // Recorded F-keys come back capitalized; saved ones may not be.
            assert_eq!(parse("F5"), parse("f5"));
        }

        #[test]
        fn garbage_falls_back_to_the_default() {
            assert_eq!(parse("nonsense"), parse(DEFAULT));
            assert_eq!(parse(""), parse(DEFAULT));
        }
    }
}

#[cfg(target_os = "macos")]
pub fn spawn_listener(tx: Sender<PipelineEvent>, settings: SharedSettings, capturing: CaptureFlag) {
    std::thread::spawn(move || {
        let mut held = false;
        let mut last_flags = macos::raw_flags();
        loop {
            let hotkey = macos::parse(&settings.read().unwrap().hotkey);
            let flags = macos::raw_flags();
            if flags != last_flags {
                log::debug!("modifier flags {last_flags:#x} -> {flags:#x}");
                last_flags = flags;
            }
            let down = macos::is_down(&hotkey);
            if down != held {
                held = down;
                // Follow the edge silently so the pipeline doesn't see a
                // half-press once recording ends.
                if capturing.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(33));
                    continue;
                }
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

/// Global key listener via rdev (Windows/Linux, once those ports land).
/// rdev gives us events rather than state, so we track what's down.
#[cfg(not(target_os = "macos"))]
mod other {
    use super::{parse_spec, DEFAULT};
    use rdev::Key;

    /// Each modifier token accepts one or more concrete keys — the
    /// side-agnostic tokens accept either side.
    fn modifier_keys(name: &str) -> &'static [Key] {
        match name {
            "fn" => &[Key::Function],
            "ctrl" => &[Key::ControlLeft, Key::ControlRight],
            "left-ctrl" => &[Key::ControlLeft],
            "right-ctrl" => &[Key::ControlRight],
            "alt" => &[Key::Alt, Key::AltGr],
            "left-alt" => &[Key::Alt],
            "right-alt" => &[Key::AltGr],
            "shift" => &[Key::ShiftLeft, Key::ShiftRight],
            "left-shift" => &[Key::ShiftLeft],
            "right-shift" => &[Key::ShiftRight],
            "cmd" => &[Key::MetaLeft, Key::MetaRight],
            "left-cmd" => &[Key::MetaLeft],
            "right-cmd" => &[Key::MetaRight],
            _ => &[],
        }
    }

    /// DOM `code` → rdev key, for the keys worth binding.
    fn key_from_code(code: &str) -> Option<Key> {
        #[rustfmt::skip]
        const KEYS: &[(&str, Key)] = &[
            ("KeyA", Key::KeyA), ("KeyB", Key::KeyB), ("KeyC", Key::KeyC),
            ("KeyD", Key::KeyD), ("KeyE", Key::KeyE), ("KeyF", Key::KeyF),
            ("KeyG", Key::KeyG), ("KeyH", Key::KeyH), ("KeyI", Key::KeyI),
            ("KeyJ", Key::KeyJ), ("KeyK", Key::KeyK), ("KeyL", Key::KeyL),
            ("KeyM", Key::KeyM), ("KeyN", Key::KeyN), ("KeyO", Key::KeyO),
            ("KeyP", Key::KeyP), ("KeyQ", Key::KeyQ), ("KeyR", Key::KeyR),
            ("KeyS", Key::KeyS), ("KeyT", Key::KeyT), ("KeyU", Key::KeyU),
            ("KeyV", Key::KeyV), ("KeyW", Key::KeyW), ("KeyX", Key::KeyX),
            ("KeyY", Key::KeyY), ("KeyZ", Key::KeyZ), ("Digit0", Key::Num0),
            ("Digit1", Key::Num1), ("Digit2", Key::Num2), ("Digit3", Key::Num3),
            ("Digit4", Key::Num4), ("Digit5", Key::Num5), ("Digit6", Key::Num6),
            ("Digit7", Key::Num7), ("Digit8", Key::Num8), ("Digit9", Key::Num9),
            ("F1", Key::F1), ("F2", Key::F2), ("F3", Key::F3), ("F4", Key::F4),
            ("F5", Key::F5), ("F6", Key::F6), ("F7", Key::F7), ("F8", Key::F8),
            ("F9", Key::F9), ("F10", Key::F10), ("F11", Key::F11), ("F12", Key::F12),
            ("F13", Key::F13), ("F14", Key::F14), ("F15", Key::F15), ("F16", Key::F16),
            ("F17", Key::F17), ("F18", Key::F18), ("F19", Key::F19), ("F20", Key::F20),
            ("Space", Key::Space), ("Enter", Key::Return), ("Tab", Key::Tab),
            ("Escape", Key::Escape), ("Backspace", Key::Backspace), ("Delete", Key::Delete),
            ("Home", Key::Home), ("End", Key::End), ("PageUp", Key::PageUp),
            ("PageDown", Key::PageDown), ("ArrowLeft", Key::LeftArrow),
            ("ArrowRight", Key::RightArrow), ("ArrowDown", Key::DownArrow),
            ("ArrowUp", Key::UpArrow), ("Minus", Key::Minus), ("Equal", Key::Equal),
            ("BracketLeft", Key::LeftBracket), ("BracketRight", Key::RightBracket),
            ("Backslash", Key::BackSlash), ("Semicolon", Key::SemiColon),
            ("Quote", Key::Quote), ("Comma", Key::Comma), ("Period", Key::Dot),
            ("Slash", Key::Slash), ("Backquote", Key::BackQuote),
        ];
        KEYS.iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(code))
            .map(|(_, key)| *key)
    }

    /// Every key the shortcut needs: one group per modifier (satisfied by any
    /// key in it) plus the optional main key in a group of its own.
    pub struct Hotkey {
        groups: Vec<Vec<Key>>,
    }

    pub fn parse(spec: &str) -> Hotkey {
        let parsed = parse_spec(spec);
        let mut groups: Vec<Vec<Key>> = parsed
            .mods
            .iter()
            .map(|name| modifier_keys(name).to_vec())
            .filter(|keys| !keys.is_empty())
            .collect();
        match parsed.key {
            Some(code) => match key_from_code(code) {
                Some(key) => groups.push(vec![key]),
                // A key we don't know: don't silently bind the modifiers alone.
                None => return fallback(spec),
            },
            None if groups.is_empty() => return fallback(spec),
            None => {}
        }
        Hotkey { groups }
    }

    fn fallback(spec: &str) -> Hotkey {
        if spec == DEFAULT {
            // Guard against a typo in DEFAULT recursing forever.
            return Hotkey {
                groups: vec![vec![Key::MetaRight]],
            };
        }
        log::warn!("unrecognized shortcut {spec:?}; falling back to {DEFAULT}");
        parse(DEFAULT)
    }

    pub fn is_down(hotkey: &Hotkey, down: &std::collections::HashSet<Key>) -> bool {
        !hotkey.groups.is_empty()
            && hotkey
                .groups
                .iter()
                .all(|group| group.iter().any(|key| down.contains(key)))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn spawn_listener(tx: Sender<PipelineEvent>, settings: SharedSettings, capturing: CaptureFlag) {
    std::thread::spawn(move || {
        let mut down = std::collections::HashSet::new();
        let mut held = false;
        let result = rdev::listen(move |event| {
            match event.event_type {
                rdev::EventType::KeyPress(key) => {
                    down.insert(key);
                }
                rdev::EventType::KeyRelease(key) => {
                    down.remove(&key);
                }
                _ => return,
            }
            let hotkey = other::parse(&settings.read().unwrap().hotkey);
            let now = other::is_down(&hotkey, &down);
            if now != held {
                held = now;
                // Follow the edge silently so the pipeline doesn't see a
                // half-press once recording ends.
                if capturing.load(Ordering::Relaxed) {
                    return;
                }
                let _ = tx.send(if now {
                    PipelineEvent::HotkeyPressed
                } else {
                    PipelineEvent::HotkeyReleased
                });
            }
        });
        if let Err(e) = result {
            log::error!("global key listener failed: {e:?}");
        }
    });
}
