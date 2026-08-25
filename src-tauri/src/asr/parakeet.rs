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
