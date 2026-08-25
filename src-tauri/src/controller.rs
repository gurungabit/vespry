use crate::asr::parakeet::ParakeetTranscriber;
use crate::asr::whisper::WhisperTranscriber;
use crate::asr::Transcriber;
use crate::audio::Recorder;
use crate::cleanup::llama::LlamaCleanup;
use crate::cleanup::{prompt, CleanupEngine};
use crate::settings::SharedSettings;
use crate::sounds::{self, Chime};
use crate::{history, hud, inject, models};
use serde::Serialize;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug)]
pub enum PipelineEvent {
    HotkeyPressed,
    HotkeyReleased,
    /// Start a hands-free session, or finish the current one (tray menu).
    Toggle,
    /// Load the ASR model into memory so the first dictation is instant.
    PreloadModel,
    /// Load the cleanup LLM into memory.
    PreloadCleanup,
}

#[derive(Clone, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
enum DictationState {
    Idle,
    Listening { hands_free: bool },
    Transcribing,
    Cleaning,
    Injecting,
    Error { message: String },
}

fn set_state(app: &AppHandle, state: DictationState) {
    let _ = app.emit("dictation-state", state);
}

/// A press-to-release shorter than this is a tap: it arms hands-free mode
/// (keep listening until the key is tapped again) instead of stopping.
const TAP_MS: u128 = 300;

/// Ignore blips shorter than this — almost certainly an accidental tap.
const MIN_SAMPLES: usize = (crate::audio::TARGET_RATE as usize) / 4;

/// Spawn the pipeline thread. It owns the recorder (whose cpal stream is
/// !Send) and the loaded ASR model, and reacts to hotkey events.
pub fn spawn(app: AppHandle, settings: SharedSettings) -> Sender<PipelineEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || run(app, settings, rx));
    tx
}

struct Pipeline {
    app: AppHandle,
    settings: SharedSettings,
    recorder: Recorder,
    transcriber: Option<Box<dyn Transcriber>>,
    /// Which engine/model/language combination `transcriber` was loaded for.
    loaded_key: String,
    cleanup: Option<LlamaCleanup>,
    listening: bool,
    hands_free: bool,
    pressed_at: Instant,
}

fn run(app: AppHandle, settings: SharedSettings, rx: Receiver<PipelineEvent>) {
    let mut p = Pipeline {
        app,
        settings,
        recorder: Recorder::new(),
        transcriber: None,
        loaded_key: String::new(),
        cleanup: None,
        listening: false,
        hands_free: false,
        pressed_at: Instant::now(),
    };

    for event in rx {
        match event {
            PipelineEvent::PreloadModel => {
                p.ensure_transcriber();
            }
            PipelineEvent::PreloadCleanup => {
                p.ensure_cleanup_loaded();
            }
            PipelineEvent::HotkeyPressed => {
                if p.listening {
                    // Second press while hands-free → stop and transcribe.
                    // (Its matching release arrives when we're no longer
                    // listening and falls through harmlessly.)
                    if p.hands_free {
                        p.finish();
                    }
                } else {
                    p.pressed_at = Instant::now();
                    p.start();
                }
            }
            PipelineEvent::Toggle => {
                if p.listening {
                    p.finish();
                } else {
                    p.start();
                    if p.listening {
                        p.hands_free = true;
                        set_state(&p.app, DictationState::Listening { hands_free: true });
                    }
                }
            }
            PipelineEvent::HotkeyReleased => {
                if !p.listening || p.hands_free {
                    continue;
                }
                if p.pressed_at.elapsed().as_millis() < TAP_MS {
                    // Quick tap → stay listening hands-free.
                    p.hands_free = true;
                    set_state(&p.app, DictationState::Listening { hands_free: true });
                } else {
                    p.finish();
                }
            }
        }
    }
}

impl Pipeline {
    fn start(&mut self) {
        match self.recorder.start(self.app.clone()) {
            Ok(()) => {
                self.listening = true;
                self.hands_free = false;
                set_state(&self.app, DictationState::Listening { hands_free: false });
                hud::show(&self.app);
                self.chime(Chime::Start);
            }
            Err(e) => self.fail(format!("couldn't start recording: {e:#}")),
        }
    }

    fn chime(&self, chime: Chime) {
        if self.settings.read().unwrap().sounds_enabled {
            sounds::play(chime);
        }
    }

    fn finish(&mut self) {
        self.listening = false;
        self.hands_free = false;
        self.chime(Chime::Stop);
        let samples = match self.recorder.stop() {
            Ok(s) => s,
            Err(e) => {
                self.fail(format!("audio capture failed: {e:#}"));
                return;
            }
        };
        if samples.len() < MIN_SAMPLES {
            self.idle();
            return;
        }
        set_state(&self.app, DictationState::Transcribing);
        self.ensure_transcriber();
        let Some(t) = self.transcriber.as_mut() else {
            self.fail("speech model unavailable".to_string());
            return;
        };
        match t.transcribe(&samples) {
            Ok(text) if text.is_empty() => self.idle(),
            Ok(text) => {
                log::info!("transcript: {text:?}");
                let raw = text.clone();
                let text = self.maybe_cleanup(text);
                let cleaned = (text != raw).then_some(text.as_str());
                history::record(&self.app, &raw, cleaned);
                set_state(&self.app, DictationState::Injecting);
                if let Err(e) = inject::inject_text(&text) {
                    self.fail(format!("couldn't insert text: {e:#}"));
                } else {
                    self.idle();
                }
            }
            Err(e) => self.fail(format!("transcription failed: {e:#}")),
        }
    }

    /// Run the transcript through the cleanup LLM if enabled and available.
    /// Any failure falls back to the raw transcript — dictation must never
    /// be lost to a flaky cleanup pass.
    fn maybe_cleanup(&mut self, text: String) -> String {
        let (enabled, dictionary) = {
            let s = self.settings.read().unwrap();
            (s.cleanup_enabled, s.dictionary.clone())
        };
        if !enabled {
            return text;
        }
        if self.cleanup.is_none() {
            if !models::qwen_installed(&self.app) {
                log::info!("cleanup model not downloaded yet; inserting raw transcript");
                return text;
            }
            set_state(&self.app, DictationState::Cleaning);
            self.ensure_cleanup_loaded();
        }
        let Some(engine) = self.cleanup.as_mut() else {
            return text;
        };
        set_state(&self.app, DictationState::Cleaning);
        let deadline = Instant::now() + Duration::from_millis(3500);
        match engine.cleanup(&text, &dictionary, deadline) {
            Ok(cleaned) if prompt::acceptable(&text, &cleaned) => cleaned,
            Ok(cleaned) => {
                log::warn!("cleanup output rejected by guardrail: {cleaned:?}");
                text
            }
            Err(e) => {
                log::warn!("cleanup failed, inserting raw transcript: {e:#}");
                text
            }
        }
    }

    fn ensure_cleanup_loaded(&mut self) {
        if self.cleanup.is_some() {
            return;
        }
        let Ok(path) = models::qwen_path(&self.app) else {
            return;
        };
        if !path.exists() {
            return;
        }
        match LlamaCleanup::load(&path) {
            Ok(engine) => self.cleanup = Some(engine),
            Err(e) => log::error!("cleanup model load failed: {e:#}"),
        }
    }

    fn idle(&self) {
        set_state(&self.app, DictationState::Idle);
        hud::hide_later(&self.app, Duration::from_millis(600));
    }

    fn fail(&self, message: String) {
        log::error!("{message}");
        set_state(
            &self.app,
            DictationState::Error {
                message: message.clone(),
            },
        );
        self.chime(Chime::Error);
        hud::hide_later(&self.app, Duration::from_millis(2200));
    }
}

impl Pipeline {
    /// Make sure the loaded transcriber matches the current settings,
    /// (re)loading and downloading models as needed.
    fn ensure_transcriber(&mut self) {
        let (engine, whisper_model, language) = {
            let s = self.settings.read().unwrap();
            (s.engine.clone(), s.whisper_model.clone(), s.language.clone())
        };
        let desired_key = match engine.as_str() {
            "whisper" => format!(
                "whisper:{whisper_model}:{}",
                language.as_deref().unwrap_or("auto")
            ),
            _ => "parakeet".to_string(),
        };
        if self.transcriber.is_some() && self.loaded_key == desired_key {
            return;
        }
        self.transcriber = None; // free the old model before loading the new one
        let loaded: Option<Box<dyn Transcriber>> = if engine == "whisper" {
            match tauri::async_runtime::block_on(models::ensure_whisper(
                &self.app,
                &whisper_model,
            )) {
                Ok(path) => match WhisperTranscriber::load(&path, language) {
                    Ok(t) => Some(Box::new(t)),
                    Err(e) => {
                        fail(&self.app, format!("whisper load failed: {e:#}"));
                        None
                    }
                },
                Err(e) => {
                    fail(&self.app, format!("whisper download failed: {e:#}"));
                    None
                }
            }
        } else {
            match tauri::async_runtime::block_on(models::ensure_parakeet(&self.app)) {
                Ok(dir) => match ParakeetTranscriber::load(&dir) {
                    Ok(t) => Some(Box::new(t)),
                    Err(e) => {
                        fail(&self.app, format!("model load failed: {e:#}"));
                        None
                    }
                },
                Err(e) => {
                    fail(&self.app, format!("model download failed: {e:#}"));
                    None
                }
            }
        };
        if loaded.is_some() {
            self.loaded_key = desired_key;
        }
        self.transcriber = loaded;
    }
}

fn fail(app: &AppHandle, message: String) {
    log::error!("{message}");
    set_state(app, DictationState::Error { message });
}
