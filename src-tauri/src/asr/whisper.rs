use super::Transcriber;
use anyhow::{Context, Result};
use std::path::Path;
use transcribe_rs::whisper_cpp::WhisperEngine;
use transcribe_rs::{SpeechModel, TranscribeOptions};

pub struct WhisperTranscriber {
    engine: WhisperEngine,
    language: Option<String>,
}

impl WhisperTranscriber {
    pub fn load(model_path: &Path, language: Option<String>) -> Result<Self> {
        let started = std::time::Instant::now();
        let engine = WhisperEngine::load(model_path)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("loading Whisper from {}", model_path.display()))?;
        log::info!("Whisper loaded in {:.2}s", started.elapsed().as_secs_f32());
        Ok(Self { engine, language })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Same say-synthesized end-to-end check as the Parakeet test.
    /// Skips (passes) if ggml-base.bin isn't downloaded.
    #[test]
    #[cfg(target_os = "macos")]
    fn transcribes_synthesized_speech() {
        let home = std::env::var("HOME").unwrap();
        let model = std::path::PathBuf::from(home)
            .join("Library/Application Support/com.vespry.app/models/ggml-base.bin");
        if !model.exists() {
            eprintln!("skipping: whisper base model not downloaded");
            return;
        }
        let wav = std::env::temp_dir().join("vespry_whisper_test.wav");
        let status = Command::new("say")
            .args([
                "-o",
                wav.to_str().unwrap(),
                "--file-format=WAVE",
                "--data-format=LEI16@16000",
                "hello world, this is a dictation test",
            ])
            .status()
            .expect("running `say`");
        assert!(status.success(), "`say` failed");

        let samples = transcribe_rs::audio::read_wav_samples(&wav).expect("reading wav");
        let mut t = WhisperTranscriber::load(&model, Some("en".into())).expect("loading model");
        let text = t.transcribe(&samples).expect("transcribing").to_lowercase();
        println!("whisper transcript: {text:?}");
        assert!(
            text.contains("hello world") && text.contains("dictation test"),
            "unexpected transcript: {text:?}"
        );
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        let started = std::time::Instant::now();
        let options = TranscribeOptions {
            language: self.language.clone(),
            ..Default::default()
        };
        let result = self
            .engine
            .transcribe(samples, &options)
            .map_err(|e| anyhow::anyhow!("whisper transcription failed: {e}"))?;
        log::info!(
            "whisper transcribed {:.2}s of audio in {:.2}s",
            samples.len() as f32 / 16_000.0,
            started.elapsed().as_secs_f32()
        );
        Ok(result.text.trim().to_string())
    }
}
