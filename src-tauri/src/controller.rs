use crate::asr::parakeet::ParakeetTranscriber;
use crate::asr::Transcriber;
use crate::audio::Recorder;
use crate::{inject, models};
use serde::Serialize;
use std::sync::mpsc::{channel, Receiver, Sender};
use tauri::{AppHandle, Emitter};

#[derive(Debug)]
pub enum PipelineEvent {
    HotkeyPressed,
    HotkeyReleased,
    /// Load the ASR model into memory so the first dictation is instant.
    PreloadModel,
}

#[derive(Clone, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
enum DictationState {
    Idle,
    Listening,
    Transcribing,
    Injecting,
    Error { message: String },
}

fn set_state(app: &AppHandle, state: DictationState) {
    let _ = app.emit("dictation-state", state);
}

/// Spawn the pipeline thread. It owns the recorder (whose cpal stream is
/// !Send) and the loaded ASR model, and reacts to hotkey events.
pub fn spawn(app: AppHandle) -> Sender<PipelineEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || run(app, rx));
    tx
}

/// Ignore blips shorter than this — almost certainly an accidental tap.
const MIN_SAMPLES: usize = (crate::audio::TARGET_RATE as usize) / 4;

fn run(app: AppHandle, rx: Receiver<PipelineEvent>) {
    let mut recorder = Recorder::new();
    let mut transcriber: Option<ParakeetTranscriber> = None;
    let mut listening = false;

    for event in rx {
        match event {
            PipelineEvent::PreloadModel => {
                if transcriber.is_none() {
                    transcriber = load_transcriber(&app);
                }
            }
            PipelineEvent::HotkeyPressed if !listening => {
                match recorder.start(app.clone()) {
                    Ok(()) => {
                        listening = true;
                        set_state(&app, DictationState::Listening);
                    }
                    Err(e) => fail(&app, format!("couldn't start recording: {e:#}")),
                }
            }
            PipelineEvent::HotkeyReleased if listening => {
                listening = false;
                let samples = match recorder.stop() {
                    Ok(s) => s,
                    Err(e) => {
                        fail(&app, format!("audio capture failed: {e:#}"));
                        continue;
                    }
                };
                if samples.len() < MIN_SAMPLES {
                    set_state(&app, DictationState::Idle);
                    continue;
                }
                set_state(&app, DictationState::Transcribing);
                if transcriber.is_none() {
                    transcriber = load_transcriber(&app);
                }
                let Some(t) = transcriber.as_mut() else {
                    fail(&app, "speech model unavailable".to_string());
                    continue;
                };
                match t.transcribe(&samples) {
                    Ok(text) if text.is_empty() => set_state(&app, DictationState::Idle),
                    Ok(text) => {
                        log::info!("transcript: {text:?}");
                        set_state(&app, DictationState::Injecting);
                        if let Err(e) = inject::inject_text(&text) {
                            fail(&app, format!("couldn't insert text: {e:#}"));
                        } else {
                            set_state(&app, DictationState::Idle);
                        }
                    }
                    Err(e) => fail(&app, format!("transcription failed: {e:#}")),
                }
            }
            _ => {}
        }
    }
}

fn fail(app: &AppHandle, message: String) {
    log::error!("{message}");
    set_state(
        app,
        DictationState::Error {
            message: message.clone(),
        },
    );
}

fn load_transcriber(app: &AppHandle) -> Option<ParakeetTranscriber> {
    // Downloads any missing model files first (no-op once installed).
    let dir = match tauri::async_runtime::block_on(models::ensure_parakeet(app)) {
        Ok(dir) => dir,
        Err(e) => {
            fail(app, format!("model download failed: {e:#}"));
            return None;
        }
    };
    match ParakeetTranscriber::load(&dir) {
        Ok(t) => Some(t),
        Err(e) => {
            fail(app, format!("model load failed: {e:#}"));
            None
        }
    }
}
