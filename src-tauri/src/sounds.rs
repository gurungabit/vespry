//! Start/stop chimes. System sounds via `afplay` for now — replaced with
//! bundled custom sounds (rodio) when the cross-platform ports land.

#[derive(Clone, Copy)]
pub enum Chime {
    Start,
    Stop,
    Error,
}

#[cfg(target_os = "macos")]
pub fn play(chime: Chime) {
    let path = match chime {
        Chime::Start => "/System/Library/Sounds/Tink.aiff",
        Chime::Stop => "/System/Library/Sounds/Pop.aiff",
        Chime::Error => "/System/Library/Sounds/Basso.aiff",
    };
    let _ = std::process::Command::new("afplay")
        .args(["-v", "0.35", path])
        .spawn();
}

#[cfg(not(target_os = "macos"))]
pub fn play(_chime: Chime) {}
