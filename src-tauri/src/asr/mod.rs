pub mod parakeet;
pub mod whisper;

use anyhow::Result;

/// A speech-to-text engine. Input is always 16 kHz mono f32 samples.
pub trait Transcriber: Send {
    fn transcribe(&mut self, samples: &[f32]) -> Result<String>;
}
