//! The floating dictation pill. On macOS it's a non-activating NSPanel so it
//! never steals focus from the app being dictated into.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::AppHandle;

pub const HUD_LABEL: &str = "hud";
const HUD_W: f64 = 300.0;
const HUD_H: f64 = 80.0;
const HUD_MARGIN: f64 = 24.0;

/// Bumped on every show/hide request so a delayed hide can tell whether a
/// newer dictation session has started in the meantime.
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use tauri::{Manager, WebviewUrl};
    use tauri_nspanel::{
        tauri_panel, CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask,
    };

    tauri_panel! {
        panel!(HudPanel {
            config: {
                can_become_key_window: false,
                can_become_main_window: false,
                is_floating_panel: true
            }
        })
    }

    fn bottom_center(app: &AppHandle) -> (f64, f64) {
        if let Ok(Some(monitor)) = app.primary_monitor() {
            let scale = monitor.scale_factor();
            let size = monitor.size().to_logical::<f64>(scale);
            let pos = monitor.position().to_logical::<f64>(scale);
            (
                pos.x + (size.width - HUD_W) / 2.0,
                pos.y + size.height - HUD_H - HUD_MARGIN,
            )
        } else {
            (400.0, 700.0)
        }
    }

    pub fn init(app: &AppHandle) -> tauri::Result<()> {
        let (x, y) = bottom_center(app);
        let panel = PanelBuilder::<_, HudPanel>::new(app, HUD_LABEL)
            .url(WebviewUrl::App("hud.html".into()))
            .size(tauri::Size::Logical(tauri::LogicalSize::new(HUD_W, HUD_H)))
            .position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)))
            .transparent(true)
            .has_shadow(false)
            .no_activate(true)
            .ignores_mouse_events(true)
            .floating(true)
            .level(PanelLevel::Status)
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary()
                    .stationary()
                    .ignores_cycle(),
            )
            .with_window(|w| w.visible(false))
            .build()?;
        panel.hide();
        Ok(())
    }

    pub fn show(app: &AppHandle) {
        let app = app.clone();
        let _ = app.clone().run_on_main_thread(move || {
            if let Some(window) = app.get_webview_window(HUD_LABEL) {
                let (x, y) = bottom_center(&app);
                let _ = window
                    .set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)));
            }
            if let Ok(panel) = app.get_webview_panel(HUD_LABEL) {
                panel.show();
            }
        });
    }

    pub fn hide(app: &AppHandle) {
        let app = app.clone();
        let _ = app.clone().run_on_main_thread(move || {
            if let Ok(panel) = app.get_webview_panel(HUD_LABEL) {
                panel.hide();
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;
    // Windows/Linux get a plain always-on-top window when those ports land.
    pub fn init(_app: &AppHandle) -> tauri::Result<()> {
        Ok(())
    }
    pub fn show(_app: &AppHandle) {}
    pub fn hide(_app: &AppHandle) {}
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    imp::init(app)
}

pub fn show(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    imp::show(app);
}

/// Hide the HUD after `delay`, unless another show/hide has been requested
/// since — lets the pill linger briefly on completion or error.
pub fn hide_later(app: &AppHandle, delay: Duration) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if GENERATION.load(Ordering::SeqCst) == generation {
            imp::hide(&app);
        }
    });
}
