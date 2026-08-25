mod asr;
mod audio;
mod controller;
mod hud;
mod inject;
mod models;
mod shortcuts;
mod sounds;

use serde::Serialize;
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    microphone: bool,
    accessibility: bool,
    model_installed: bool,
}

#[tauri::command]
async fn get_status(app: AppHandle) -> Status {
    #[cfg(target_os = "macos")]
    let (microphone, accessibility) = (
        tauri_plugin_macos_permissions::check_microphone_permission().await,
        tauri_plugin_macos_permissions::check_accessibility_permission().await,
    );
    #[cfg(not(target_os = "macos"))]
    let (microphone, accessibility) = (true, true);
    Status {
        microphone,
        accessibility,
        model_installed: models::parakeet_installed(&app),
    }
}

#[tauri::command]
async fn request_permission(name: String) {
    #[cfg(target_os = "macos")]
    match name.as_str() {
        "microphone" => {
            let _ = tauri_plugin_macos_permissions::request_microphone_permission().await;
        }
        "accessibility" => {
            tauri_plugin_macos_permissions::request_accessibility_permission().await;
        }
        _ => {}
    }
    #[cfg(not(target_os = "macos"))]
    let _ = name;
}

#[cfg(target_os = "macos")]
async fn request_permissions_on_launch() {
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

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_macos_permissions::init());
    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .invoke_handler(tauri::generate_handler![get_status, request_permission])
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

            // The floating dictation pill (hidden until dictation starts).
            hud::init(app.handle())?;

            // The dictation pipeline thread + the global hotkey listener.
            let pipeline = controller::spawn(app.handle().clone());
            shortcuts::spawn_listener(pipeline.clone());

            // Ask for mic/accessibility up front, then fetch + preload the ASR
            // model so the first dictation is instant.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                #[cfg(target_os = "macos")]
                request_permissions_on_launch().await;
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
