mod asr;
mod audio;
mod controller;
mod inject;
mod models;
mod shortcuts;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(target_os = "macos")]
async fn request_permissions() {
    use tauri_plugin_macos_permissions as perms;
    if !perms::check_microphone_permission().await {
        let _ = perms::request_microphone_permission().await;
    }
    if !perms::check_accessibility_permission().await {
        log::warn!("Accessibility not granted — hotkey and paste won't work until it is");
        perms::request_accessibility_permission().await;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .setup(|app| {
            // Menu-bar app: no Dock icon, lives in the tray.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings =
                MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Vespry", true, Some("CmdOrCtrl+Q"))?;
            let menu = Menu::with_items(app, &[&settings, &separator, &quit])?;

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // The dictation pipeline thread + the global hotkey listener.
            let pipeline = controller::spawn(app.handle().clone());
            shortcuts::spawn_listener(pipeline.clone());

            // Ask for mic/accessibility up front, then fetch + preload the ASR
            // model so the first dictation is instant.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                #[cfg(target_os = "macos")]
                request_permissions().await;
                match models::ensure_parakeet(&handle).await {
                    Ok(_) => {
                        let _ = pipeline.send(controller::PipelineEvent::PreloadModel);
                    }
                    Err(e) => log::error!("model download failed: {e:#}"),
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the settings window hides it; the app stays in the tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
