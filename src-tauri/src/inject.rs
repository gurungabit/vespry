use anyhow::{Context, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

/// Type `text` into the focused app: stash the clipboard, paste, restore.
///
/// The synthetic ⌘V must run on the main thread: enigo resolves the layout-
/// dependent keycode through the Text Services Manager, which dispatch-asserts
/// the main queue on modern macOS (SIGTRAP off-main). Clipboard work stays on
/// the calling thread.
///
/// Restoring only preserves plain text — images or rich content on the
/// clipboard are lost. Fine for now; revisit if it bites.
pub fn inject_text(app: &AppHandle, text: &str) -> Result<()> {
    log::info!("inserting {} chars", text.chars().count());
    let mut clipboard = arboard::Clipboard::new().context("opening clipboard")?;
    let saved = clipboard.get_text().ok();

    clipboard
        .set_text(text.to_string())
        .context("writing transcript to clipboard")?;
    // Give the pasteboard a beat to settle before the keystroke.
    thread::sleep(Duration::from_millis(60));

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = done_tx.send(paste_keystroke());
    })
    .context("scheduling paste on main thread")?;
    done_rx
        .recv_timeout(Duration::from_secs(3))
        .context("paste keystroke timed out")?
        .context("synthesizing paste keystroke")?;

    // Let the target app read the clipboard before we restore it.
    thread::sleep(Duration::from_millis(350));
    if let Some(previous) = saved {
        let _ = clipboard.set_text(previous);
    }
    Ok(())
}

fn paste_keystroke() -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("enigo init (accessibility granted?): {e}"))?;
    let modifier = if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    };
    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let result = enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| anyhow::anyhow!("{e}"));
    // Always release the modifier, even if the 'v' failed.
    let _ = enigo.key(modifier, Direction::Release);
    result?;
    Ok(())
}
