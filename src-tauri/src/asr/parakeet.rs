use super::Transcriber;
use anyhow::{Context, Result};
use std::path::Path;
use transcribe_rs::onnx::parakeet::ParakeetModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

pub struct ParakeetTranscriber {
    model: ParakeetModel,
}

impl ParakeetTranscriber {
    pub fn load(model_dir: &Path) -> Result<Self> {
        let started = std::time::Instant::now();
        let model = ParakeetModel::load(model_dir, &Quantization::Int8)
            .with_context(|| format!("loading Parakeet from {}", model_dir.display()))?;
        log::info!("Parakeet loaded in {:.2}s", started.elapsed().as_secs_f32());
        Ok(Self { model })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// End-to-end ASR check without a microphone: synthesize speech with the
    /// macOS `say` command and run it through Parakeet. Skips (passes) if the
    /// model hasn't been downloaded yet — run the app once first.
    #[test]
    #[cfg(target_os = "macos")]
    fn transcribes_synthesized_speech() {
        let home = std::env::var("HOME").unwrap();
        let model_dir = std::path::PathBuf::from(home)
            .join("Library/Application Support/com.vespry.app/models")
            .join(crate::models::PARAKEET_DIR);
        if !model_dir.join("vocab.txt").exists() {
            eprintln!("skipping: Parakeet model not downloaded");
            return;
        }
        let wav = std::env::temp_dir().join("vespry_asr_test.wav");
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
        let mut t = ParakeetTranscriber::load(&model_dir).expect("loading model");
        let text = t.transcribe(&samples).expect("transcribing").to_lowercase();
        println!("transcript: {text:?}");
        assert!(
            text.contains("hello world") && text.contains("dictation test"),
            "unexpected transcript: {text:?}"
        );
    }
}

impl Transcriber for ParakeetTranscriber {
    fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        let started = std::time::Instant::now();
        let result = self
            .model
            .transcribe(samples, &TranscribeOptions::default())
            .map_err(|e| anyhow::anyhow!("parakeet transcription failed: {e}"))?;
        log::info!(
            "transcribed {:.2}s of audio in {:.2}s",
            samples.len() as f32 / 16_000.0,
            started.elapsed().as_secs_f32()
        );
        Ok(result.text.trim().to_string())
    }
}
