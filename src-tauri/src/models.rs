use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

pub const PARAKEET_DIR: &str = "parakeet-tdt-0.6b-v3-int8";
const DEFAULT_HF_ENDPOINT: &str = "https://huggingface.co";
const PARAKEET_REPO: &str = "istupakov/parakeet-tdt-0.6b-v3-onnx";
const PARAKEET_FILES: &[&str] = &[
    "encoder-model.int8.onnx",
    "decoder_joint-model.int8.onnx",
    "nemo128.onnx",
    "vocab.txt",
];

// Cleanup LLM. The 1.7B Qwen3 was too weak — it either executed transcript
// fragments as instructions or stopped removing fillers; 4B-Instruct holds
// both rules and is still sub-second on Apple Silicon.
pub const QWEN_NAME: &str = "qwen3-4b-instruct-q4km";
const QWEN_FILE: &str = "Qwen3-4B-Instruct-2507-Q4_K_M.gguf";
const QWEN_REPO: &str = "unsloth/Qwen3-4B-Instruct-2507-GGUF";

/// Curated whisper.cpp models (multilingual, ggml format).
pub struct WhisperModel {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub size_mb: u32,
}

pub const WHISPER_MODELS: &[WhisperModel] = &[
    WhisperModel {
        id: "base",
        label: "Whisper Base — fastest, rough",
        file: "ggml-base.bin",
        size_mb: 142,
    },
    WhisperModel {
        id: "small",
        label: "Whisper Small — balanced",
        file: "ggml-small.bin",
        size_mb: 466,
    },
    WhisperModel {
        id: "large-v3-turbo-q5_0",
        label: "Whisper Large v3 Turbo (q5) — best quality",
        file: "ggml-large-v3-turbo-q5_0.bin",
        size_mb: 547,
    },
];

const WHISPER_REPO: &str = "ggerganov/whisper.cpp";

/// Pick the download host: runtime environment first (so a launch-time
/// override always wins), then the Settings field, then any endpoint baked in
/// at build time, then Hugging Face itself. Blank or whitespace-only values
/// are ignored so an empty setting or `VESPRY_HF_ENDPOINT=` can't produce a
/// malformed URL.
fn resolve_endpoint(configured: &str) -> String {
    fn non_blank(value: String) -> Option<String> {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }

    std::env::var("VESPRY_HF_ENDPOINT")
        .ok()
        .and_then(non_blank)
        .or_else(|| non_blank(configured.to_owned()))
        .or_else(|| {
            option_env!("VESPRY_HF_ENDPOINT")
                .map(str::to_owned)
                .and_then(non_blank)
        })
        .unwrap_or_else(|| DEFAULT_HF_ENDPOINT.to_owned())
}

fn model_url(app: &AppHandle, repository: &str, file: &str) -> String {
    let configured = crate::settings::load(app).hf_endpoint;
    format!(
        "{}/{repository}/resolve/main/{file}",
        resolve_endpoint(&configured).trim_end_matches('/')
    )
}

pub fn whisper_model(id: &str) -> Option<&'static WhisperModel> {
    WHISPER_MODELS.iter().find(|m| m.id == id)
}

pub fn whisper_path(app: &AppHandle, id: &str) -> Result<PathBuf> {
    let model = whisper_model(id).context("unknown whisper model")?;
    Ok(models_root(app)?.join(model.file))
}

pub fn whisper_installed(app: &AppHandle, id: &str) -> bool {
    whisper_path(app, id).map(|p| p.exists()).unwrap_or(false)
}

/// Download a whisper model if missing, then return its path.
pub async fn ensure_whisper(app: &AppHandle, id: &str) -> Result<PathBuf> {
    let model = whisper_model(id).context("unknown whisper model")?;
    let path = whisper_path(app, id)?;
    if path.exists() {
        return Ok(path);
    }
    tokio::fs::create_dir_all(models_root(app)?).await?;
    log::info!("downloading {}…", model.file);
    let client = reqwest::Client::new();
    download(
        &client,
        &model_url(app, WHISPER_REPO, model.file),
        &path,
        app,
        id,
        model.file,
    )
    .await
    .with_context(|| format!("downloading {}", model.file))?;
    Ok(path)
}

#[derive(Clone, Serialize)]
struct DownloadProgress<'a> {
    model: &'a str,
    file: &'a str,
    downloaded: u64,
    total: Option<u64>,
    done: bool,
}

pub fn models_root(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .context("no app data dir")?
        .join("models"))
}

pub fn parakeet_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(models_root(app)?.join(PARAKEET_DIR))
}

#[allow(dead_code)] // wired to the Models settings tab in M2
pub fn parakeet_installed(app: &AppHandle) -> bool {
    parakeet_dir(app)
        .map(|dir| PARAKEET_FILES.iter().all(|f| dir.join(f).exists()))
        .unwrap_or(false)
}

pub fn qwen_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(models_root(app)?.join(QWEN_FILE))
}

pub fn qwen_installed(app: &AppHandle) -> bool {
    qwen_path(app).map(|p| p.exists()).unwrap_or(false)
}

/// Download any missing Parakeet model files, then return the model directory.
/// Progress is emitted as `model-download` events for the UI.
pub async fn ensure_parakeet(app: &AppHandle) -> Result<PathBuf> {
    let dir = parakeet_dir(app)?;
    tokio::fs::create_dir_all(&dir).await?;
    let client = reqwest::Client::new();
    for file in PARAKEET_FILES {
        let dest = dir.join(file);
        if dest.exists() {
            continue;
        }
        log::info!("downloading {file}…");
        download(
            &client,
            &model_url(app, PARAKEET_REPO, file),
            &dest,
            app,
            PARAKEET_DIR,
            file,
        )
        .await
        .with_context(|| format!("downloading {file}"))?;
    }
    Ok(dir)
}

/// Download the cleanup LLM if missing, then return its path.
pub async fn ensure_qwen(app: &AppHandle) -> Result<PathBuf> {
    let path = qwen_path(app)?;
    if path.exists() {
        return Ok(path);
    }
    tokio::fs::create_dir_all(models_root(app)?).await?;
    log::info!("downloading {QWEN_FILE}…");
    let client = reqwest::Client::new();
    download(
        &client,
        &model_url(app, QWEN_REPO, QWEN_FILE),
        &path,
        app,
        QWEN_NAME,
        QWEN_FILE,
    )
    .await
    .with_context(|| format!("downloading {QWEN_FILE}"))?;
    Ok(path)
}

/// One lock per destination file. Selecting an uninstalled model both saves
/// the setting (which preloads, downloading) and calls download_model, so the
/// same file could be fetched twice at once: whichever finished first renamed
/// the shared .part file and the other failed with ENOENT.
fn download_lock(dest: &PathBuf) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry(dest.clone())
        .or_default()
        .clone()
}

async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &PathBuf,
    app: &AppHandle,
    model: &str,
    file: &str,
) -> Result<()> {
    let lock = download_lock(dest);
    let _guard = lock.lock().await;
    if dest.exists() {
        // A concurrent request finished this file while we waited.
        return Ok(());
    }

    let mut request = client.get(url);
    if let Ok(token) = std::env::var("VESPRY_HF_TOKEN").or_else(|_| std::env::var("HF_TOKEN")) {
        request = request.bearer_auth(token);
    }
    let resp = request.send().await?.error_for_status()?;
    // A captive portal or a mirror serving a login page answers 200 with HTML,
    // which would otherwise be written out as a corrupt "model" file.
    if resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html"))
    {
        anyhow::bail!(
            "model download returned HTML instead of model data; set VESPRY_HF_ENDPOINT to an accessible Hugging Face mirror"
        );
    }
    let total = resp.content_length();
    // Write to a .part file so an interrupted download never looks installed.
    let part = dest.with_extension("part");
    let mut out = tokio::fs::File::create(&part).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        out.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed().as_millis() > 200 {
            last_emit = std::time::Instant::now();
            let _ = app.emit(
                "model-download",
                DownloadProgress {
                    model,
                    file,
                    downloaded,
                    total,
                    done: false,
                },
            );
        }
    }
    out.flush().await?;
    drop(out);
    tokio::fs::rename(&part, dest).await?;
    let _ = app.emit(
        "model-download",
        DownloadProgress {
            model,
            file,
            downloaded,
            total,
            done: true,
        },
    );
    log::info!("downloaded {file} ({downloaded} bytes)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Endpoint resolution is env-sensitive, so these run in one test to keep
    /// the var's effects ordered (cargo runs tests in parallel threads).
    #[test]
    fn endpoint_precedence_and_blank_handling() {
        // SAFETY: single-threaded within this test; no other test reads this var.
        unsafe { std::env::remove_var("VESPRY_HF_ENDPOINT") };
        assert_eq!(resolve_endpoint(""), DEFAULT_HF_ENDPOINT);
        assert_eq!(resolve_endpoint("   "), DEFAULT_HF_ENDPOINT);
        assert_eq!(
            resolve_endpoint("https://mirror.example.com"),
            "https://mirror.example.com"
        );
        // Pasted values often carry whitespace.
        assert_eq!(
            resolve_endpoint("  https://mirror.example.com  "),
            "https://mirror.example.com"
        );

        unsafe { std::env::set_var("VESPRY_HF_ENDPOINT", "https://env.example.com") };
        assert_eq!(
            resolve_endpoint("https://setting.example.com"),
            "https://env.example.com"
        );

        // An explicitly empty env var must not win and produce a bare path.
        unsafe { std::env::set_var("VESPRY_HF_ENDPOINT", "") };
        assert_eq!(
            resolve_endpoint("https://setting.example.com"),
            "https://setting.example.com"
        );
        unsafe { std::env::remove_var("VESPRY_HF_ENDPOINT") };
    }
}
