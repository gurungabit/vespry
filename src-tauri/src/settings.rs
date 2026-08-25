use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Run transcripts through the local LLM before inserting.
    pub cleanup_enabled: bool,
    /// Names/terms the cleanup model should prefer the given spellings of.
    pub dictionary: Vec<String>,
    /// ASR engine: "parakeet" (default) or "whisper".
    pub engine: String,
    /// Which whisper model to use when engine is "whisper".
    pub whisper_model: String,
    /// Spoken-language hint for whisper (BCP-47, e.g. "ja"); None = auto-detect.
    pub language: Option<String>,
    /// Push-to-talk key id: right-cmd, right-alt, fn, left-ctrl, f5.
    pub hotkey: String,
    /// Play start/stop chimes.
    pub sounds_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cleanup_enabled: true,
            dictionary: Vec::new(),
            engine: "parakeet".into(),
            whisper_model: "small".into(),
            language: None,
            hotkey: "right-cmd".into(),
            sounds_enabled: true,
        }
    }
}

pub type SharedSettings = Arc<RwLock<Settings>>;

fn path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .context("no app data dir")?
        .join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    let Ok(path) = path(app) else {
        return Settings::default();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<()> {
    let path = path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}
