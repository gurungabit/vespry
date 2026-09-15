pub mod llama;
pub mod prompt;

use anyhow::Result;
use std::time::Instant;

/// Turns a raw transcript into clean written text. Implementations must
/// respect `deadline` — past it, stop generating and return an error.
/// Only completed output is usable; the caller falls back to the raw transcript
/// on failure rather than risking a truncated dictation.
pub trait CleanupEngine: Send {
    fn cleanup(
        &mut self,
        transcript: &str,
        dictionary: &[String],
        smart_formatting: bool,
        deadline: Instant,
    ) -> Result<String>;
}
