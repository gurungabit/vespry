use crate::asr::parakeet::ParakeetTranscriber;
use crate::asr::Transcriber;
use crate::audio::Recorder;
use crate::sounds::{self, Chime};
use crate::{hud, inject, models};
use serde::Serialize;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
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
    Listening { hands_free: bool },
    Transcribing,
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
pub fn spawn(app: AppHandle) -> Sender<PipelineEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || run(app, rx));
    tx
}

struct Pipeline {
    app: AppHandle,
    recorder: Recorder,
    transcriber: Option<ParakeetTranscriber>,
    listening: bool,
    hands_free: bool,
    pressed_at: Instant,
}

fn run(app: AppHandle, rx: Receiver<PipelineEvent>) {
    let mut p = Pipeline {
        app,
        recorder: Recorder::new(),
        transcriber: None,
        listening: false,
        hands_free: false,
        pressed_at: Instant::now(),
    };

    for event in rx {
        match event {
            PipelineEvent::PreloadModel => {
                if p.transcriber.is_none() {
                    p.transcriber = load_transcriber(&p.app);
                }
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
                sounds::play(Chime::Start);
            }
            Err(e) => self.fail(format!("couldn't start recording: {e:#}")),
        }
    }

    fn finish(&mut self) {
        self.listening = false;
        self.hands_free = false;
        sounds::play(Chime::Stop);
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
        if self.transcriber.is_none() {
            self.transcriber = load_transcriber(&self.app);
        }
        let Some(t) = self.transcriber.as_mut() else {
            self.fail("speech model unavailable".to_string());
            return;
        };
        match t.transcribe(&samples) {
            Ok(text) if text.is_empty() => self.idle(),
            Ok(text) => {
                log::info!("transcript: {text:?}");
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
        sounds::play(Chime::Error);
        hud::hide_later(&self.app, Duration::from_millis(2200));
    }
}

fn fail(app: &AppHandle, message: String) {
    log::error!("{message}");
    set_state(app, DictationState::Error { message });
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
