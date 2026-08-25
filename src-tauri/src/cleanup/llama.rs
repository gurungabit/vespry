use super::{prompt, CleanupEngine};
use anyhow::{anyhow, Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use std::num::NonZeroU32;
use std::path::Path;
use std::time::Instant;

const N_CTX: u32 = 4096;
const MAX_NEW_TOKENS: usize = 512;

pub struct LlamaCleanup {
    backend: LlamaBackend,
    model: LlamaModel,
}

impl LlamaCleanup {
    pub fn load(gguf_path: &Path) -> Result<Self> {
        let started = Instant::now();
        let backend = LlamaBackend::init().context("llama backend init")?;
        // Offload everything to Metal.
        let params = LlamaModelParams::default().with_n_gpu_layers(1_000_000);
        let model = LlamaModel::load_from_file(&backend, gguf_path, &params)
            .with_context(|| format!("loading {}", gguf_path.display()))?;
        log::info!("cleanup LLM loaded in {:.2}s", started.elapsed().as_secs_f32());
        Ok(Self { backend, model })
    }

    fn generate(&mut self, full_prompt: &str, deadline: Instant) -> Result<String> {
        // Fresh context per request: cheap, and no KV-cache state to manage.
        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX));
        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .context("creating llama context")?;

        let tokens = self
            .model
            .str_to_token(full_prompt, AddBos::Never)
            .map_err(|e| anyhow!("tokenize: {e}"))?;
        if tokens.len() as u32 + 64 > N_CTX {
            return Err(anyhow!("transcript too long for cleanup context"));
        }

        let mut batch = LlamaBatch::new(tokens.len().max(64), 1);
        let last = tokens.len() as i32 - 1;
        for (i, token) in (0i32..).zip(tokens.iter()) {
            batch.add(*token, i, &[0], i == last)?;
        }
        ctx.decode(&mut batch).map_err(|e| anyhow!("prefill: {e}"))?;

        // Greedy: cleanup is a deterministic rewriting task, not creative writing.
        let mut sampler = LlamaSampler::greedy();

        let mut out = String::new();
        let mut n_cur = batch.n_tokens();
        let max_tokens = (tokens.len() * 2).clamp(64, MAX_NEW_TOKENS);
        for _ in 0..max_tokens {
            if Instant::now() > deadline {
                log::warn!("cleanup hit its deadline; returning partial output");
                break;
            }
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            if self.model.is_eog_token(token) {
                break;
            }
            let piece = self
                .model
                .token_to_str(token, Special::Plaintext)
                .unwrap_or_default();
            out.push_str(&piece);
            batch.clear();
            batch.add(token, n_cur, &[0], true)?;
            n_cur += 1;
            ctx.decode(&mut batch).map_err(|e| anyhow!("decode: {e}"))?;
        }
        Ok(out)
    }
}

impl CleanupEngine for LlamaCleanup {
    fn cleanup(
        &mut self,
        transcript: &str,
        dictionary: &[String],
        deadline: Instant,
    ) -> Result<String> {
        let started = Instant::now();
        let full_prompt =
            prompt::build_chatml(&prompt::system_prompt(dictionary), transcript);
        let raw = self.generate(&full_prompt, deadline)?;
        let cleaned = prompt::postprocess(&raw);
        log::info!(
            "cleanup took {:.2}s ({} chars -> {} chars)",
            started.elapsed().as_secs_f32(),
            transcript.len(),
            cleaned.len()
        );
        Ok(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// End-to-end cleanup check. Skips (passes) if the GGUF isn't downloaded
    /// yet — run the app once with cleanup enabled first.
    #[test]
    fn cleans_filler_ridden_transcript() {
        let home = std::env::var("HOME").unwrap();
        let gguf = std::path::PathBuf::from(home)
            .join("Library/Application Support/com.vespry.app/models/Qwen3-1.7B-Q4_K_M.gguf");
        if !gguf.exists() {
            eprintln!("skipping: cleanup model not downloaded");
            return;
        }
        let mut engine = LlamaCleanup::load(&gguf).expect("loading model");
        let transcript =
            "um so basically i think we should uh we should ship the the feature tomorrow you know";
        let cleaned = engine
            .cleanup(transcript, &[], Instant::now() + Duration::from_secs(30))
            .expect("cleanup");
        println!("cleaned: {cleaned:?}");
        let lower = cleaned.to_lowercase();
        assert!(lower.contains("ship the feature tomorrow"), "got: {cleaned:?}");
        for filler in ["um", "uh ", "you know", "basically"] {
            assert!(!lower.contains(filler), "filler {filler:?} survived: {cleaned:?}");
        }
        assert!(
            cleaned.chars().next().unwrap().is_uppercase() && cleaned.ends_with('.'),
            "not sentence-cased: {cleaned:?}"
        );
        assert!(prompt::acceptable(transcript, &cleaned));
    }
}
