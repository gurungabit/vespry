mod asr;
mod audio;
mod cleanup;
mod controller;
mod history;
mod hud;
mod inject;
mod models;
mod settings;
mod shortcuts;
mod sounds;

use settings::{Settings, SharedSettings};
use std::sync::{Arc, RwLock};

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
    cleanup_model_installed: bool,
}

#[tauri::command]
fn get_settings(state: tauri::State<SharedSettings>) -> Settings {
    state.read().unwrap().clone()
}

#[tauri::command]
fn set_settings(
    app: AppHandle,
    state: tauri::State<SharedSettings>,
    new_settings: Settings,
) -> Result<(), String> {
    settings::save(&app, &new_settings).map_err(|e| e.to_string())?;
    *state.write().unwrap() = new_settings;
    // Reload models eagerly so an engine switch doesn't stall the next dictation.
    app.state::<PipelineHandle>()
        .send(controller::PipelineEvent::PreloadModel);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelInfo {
    id: String,
    label: String,
    size_mb: u32,
    installed: bool,
    kind: String,
}

#[tauri::command]
fn list_models(app: AppHandle) -> Vec<ModelInfo> {
    let mut list = vec![ModelInfo {
        id: "parakeet".into(),
        label: "Parakeet v3 — fastest, 25 European languages".into(),
        size_mb: 670,
        installed: models::parakeet_installed(&app),
        kind: "asr".into(),
    }];
    for m in models::WHISPER_MODELS {
        list.push(ModelInfo {
            id: m.id.into(),
            label: m.label.into(),
            size_mb: m.size_mb,
            installed: models::whisper_installed(&app, m.id),
            kind: "asr".into(),
        });
    }
    list.push(ModelInfo {
        id: "qwen".into(),
        label: "Qwen3 1.7B — transcript cleanup".into(),
        size_mb: 1056,
        installed: models::qwen_installed(&app),
        kind: "cleanup".into(),
    });
    list
}

#[tauri::command]
fn get_history(app: AppHandle) -> Vec<history::HistoryEntry> {
    history::load(&app)
}

#[tauri::command]
fn delete_history_entry(app: AppHandle, at: u64) -> Result<(), String> {
    history::delete(&app, at).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_history(app: AppHandle) -> Result<(), String> {
    history::clear(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(|e| e.to_string())
    } else {
        autolaunch.disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    match id.as_str() {
        "parakeet" => models::ensure_parakeet(&app).await.map(|_| ()),
        "qwen" => models::ensure_qwen(&app).await.map(|_| ()),
        other => models::ensure_whisper(&app, other).await.map(|_| ()),
    }
    .map_err(|e| e.to_string())
}

/// Fetch the cleanup model on demand (e.g. when the user flips the toggle
/// before it ever downloaded), then warm it up.
/// The pipeline's event sender, shareable across commands (mpsc Sender is !Sync).
struct PipelineHandle(std::sync::Mutex<std::sync::mpsc::Sender<controller::PipelineEvent>>);

impl PipelineHandle {
    fn send(&self, event: controller::PipelineEvent) {
        let _ = self.0.lock().unwrap().send(event);
    }
}

/// Fetch the cleanup model on demand (e.g. when the user flips the toggle
/// before it ever downloaded), then warm it up.
#[tauri::command]
async fn download_cleanup_model(
    app: AppHandle,
    pipeline: tauri::State<'_, PipelineHandle>,
) -> Result<(), String> {
    models::ensure_qwen(&app).await.map_err(|e| e.to_string())?;
    pipeline.send(controller::PipelineEvent::PreloadCleanup);
    Ok(())
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
        cleanup_model_installed: models::qwen_installed(&app),
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
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_macos_permissions::init());
    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .invoke_handler(tauri::generate_handler![
            get_status,
            request_permission,
            get_settings,
            set_settings,
            download_cleanup_model,
            list_models,
            download_model,
            get_history,
            delete_history_entry,
            clear_history,
            get_autostart,
            set_autostart
        ])
        .setup(|app| {
            // Menu-bar app: no Dock icon, lives in the tray.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let shared_settings: SharedSettings =
                Arc::new(RwLock::new(settings::load(app.handle())));
            app.manage(shared_settings.clone());

            // The dictation pipeline thread + the global hotkey listener.
            let pipeline = controller::spawn(app.handle().clone(), shared_settings.clone());
            shortcuts::spawn_listener(pipeline.clone(), shared_settings.clone());
            app.manage(PipelineHandle(std::sync::Mutex::new(pipeline.clone())));

            let toggle = MenuItem::with_id(app, "toggle", "Start / Stop Dictation", true, None::<&str>)?;
            let settings =
                MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Vespry", true, Some("CmdOrCtrl+Q"))?;
            let menu = Menu::with_items(app, &[&toggle, &settings, &separator, &quit])?;

            let tray_pipeline = pipeline.clone();
            TrayIconBuilder::with_id("main")
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../icons/tray.png"
                ))?)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "toggle" => {
                        let _ = tray_pipeline.send(controller::PipelineEvent::Toggle);
                    }
                    "settings" => show_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // The floating dictation pill (hidden until dictation starts).
            // Created after launch finishes: building a webview panel inside
            // setup() runs during applicationDidFinishLaunching, where a
            // panic/objc exception aborts the process.
            let hud_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                let inner = hud_handle.clone();
                let _ = hud_handle.run_on_main_thread(move || {
                    log::info!("initializing HUD panel…");
                    match hud::init(&inner) {
                        Ok(()) => log::info!("HUD panel ready"),
                        Err(e) => log::error!("HUD init failed: {e}"),
                    }
                });
            });

            // Ask for mic/accessibility up front, then fetch + preload the ASR
            // model so the first dictation is instant.
            let handle = app.handle().clone();
            let cleanup_enabled = shared_settings.read().unwrap().cleanup_enabled;
            tauri::async_runtime::spawn(async move {
                #[cfg(target_os = "macos")]
                request_permissions_on_launch().await;
                match models::ensure_parakeet(&handle).await {
                    Ok(_) => {
                        let _ = pipeline.send(controller::PipelineEvent::PreloadModel);
                    }
                    Err(e) => log::error!("model download failed: {e:#}"),
                }
                if cleanup_enabled {
                    match models::ensure_qwen(&handle).await {
                        Ok(_) => {
                            let _ = pipeline.send(controller::PipelineEvent::PreloadCleanup);
                        }
                        Err(e) => log::error!("cleanup model download failed: {e:#}"),
                    }
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
