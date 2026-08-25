use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

pub const PARAKEET_DIR: &str = "parakeet-tdt-0.6b-v3-int8";
const PARAKEET_BASE: &str =
    "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main";
const PARAKEET_FILES: &[&str] = &[
    "encoder-model.int8.onnx",
    "decoder_joint-model.int8.onnx",
    "nemo128.onnx",
    "vocab.txt",
];

pub const QWEN_NAME: &str = "qwen3-1.7b-q4km";
const QWEN_FILE: &str = "Qwen3-1.7B-Q4_K_M.gguf";
const QWEN_URL: &str =
    "https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf";

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

const WHISPER_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

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
        &format!("{WHISPER_BASE_URL}/{}", model.file),
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
            &format!("{PARAKEET_BASE}/{file}"),
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
    download(&client, QWEN_URL, &path, app, QWEN_NAME, QWEN_FILE)
        .await
        .with_context(|| format!("downloading {QWEN_FILE}"))?;
    Ok(path)
}

async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &PathBuf,
    app: &AppHandle,
    model: &str,
    file: &str,
) -> Result<()> {
    let resp = client.get(url).send().await?.error_for_status()?;
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
