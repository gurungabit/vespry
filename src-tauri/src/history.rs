use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const MAX_ENTRIES: usize = 500;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// Unix millis.
    pub at: u64,
    pub raw: String,
    /// Present when the cleanup pass changed the text.
    pub cleaned: Option<String>,
}

fn path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .context("no app data dir")?
        .join("history.json"))
}

pub fn load(app: &AppHandle) -> Vec<HistoryEntry> {
    let Ok(path) = path(app) else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn store(app: &AppHandle, entries: &[HistoryEntry]) -> Result<()> {
    let path = path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, serde_json::to_string(entries)?)?;
    Ok(())
}

/// Append an entry, newest first, capped at MAX_ENTRIES.
pub fn record(app: &AppHandle, raw: &str, cleaned: Option<&str>) {
    let mut entries = load(app);
    entries.insert(
        0,
        HistoryEntry {
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            raw: raw.to_string(),
            cleaned: cleaned.map(str::to_string),
        },
    );
    entries.truncate(MAX_ENTRIES);
    if let Err(e) = store(app, &entries) {
        log::warn!("couldn't save history: {e:#}");
    }
}

pub fn delete(app: &AppHandle, at: u64) -> Result<()> {
    let mut entries = load(app);
    entries.retain(|e| e.at != at);
    store(app, &entries)
}

pub fn clear(app: &AppHandle) -> Result<()> {
    store(app, &[])
}
