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
use std::sync::OnceLock;
use std::time::Instant;

const N_CTX: u32 = 4096;
const MAX_NEW_TOKENS: usize = 512;

/// llama.cpp's backend is process-global — initializing it a second time
/// (reloading the model, or a second engine in one process) fails.
fn backend() -> Result<&'static LlamaBackend> {
    // The init must happen inside get_or_init: a check-then-init would let two
    // threads race into a second llama_backend_init.
    static BACKEND: OnceLock<std::result::Result<LlamaBackend, String>> = OnceLock::new();
    BACKEND
        .get_or_init(|| LlamaBackend::init().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| anyhow!("llama backend init: {e}"))
}

pub struct LlamaCleanup {
    backend: &'static LlamaBackend,
    model: LlamaModel,
}

impl LlamaCleanup {
    pub fn load(gguf_path: &Path) -> Result<Self> {
        let started = Instant::now();
        let backend = backend()?;
        // Offload everything to Metal.
        let params = LlamaModelParams::default().with_n_gpu_layers(1_000_000);
        let model = LlamaModel::load_from_file(backend, gguf_path, &params)
            .with_context(|| format!("loading {}", gguf_path.display()))?;
        log::info!(
            "cleanup LLM loaded in {:.2}s",
            started.elapsed().as_secs_f32()
        );
        Ok(Self { backend, model })
    }

    fn generate(&mut self, full_prompt: &str, deadline: Instant) -> Result<String> {
        if Instant::now() >= deadline {
            return Err(anyhow!("cleanup deadline exceeded"));
        }
        // Fresh context per request: cheap, and no KV-cache state to manage.
        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX));
        let mut ctx = self
            .model
            .new_context(self.backend, ctx_params)
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
        ctx.decode(&mut batch)
            .map_err(|e| anyhow!("prefill: {e}"))?;

        // Greedy: cleanup is a deterministic rewriting task, not creative writing.
        let mut sampler = LlamaSampler::greedy();

        let mut out = String::new();
        let mut n_cur = batch.n_tokens();
        let max_tokens = (tokens.len() * 2)
            .clamp(64, MAX_NEW_TOKENS)
            .min(N_CTX as usize - tokens.len());
        for _ in 0..max_tokens {
            if Instant::now() >= deadline {
                return Err(anyhow!("cleanup deadline exceeded"));
            }
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            if self.model.is_eog_token(token) {
                return Ok(out);
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
        // A partial answer can pass the word-overlap guardrail while losing
        // the final constraint. Let the caller fall back to the full transcript.
        Err(anyhow!("cleanup reached its token limit before completing"))
    }
}

impl CleanupEngine for LlamaCleanup {
    fn cleanup(
        &mut self,
        transcript: &str,
        dictionary: &[String],
        smart_formatting: bool,
        deadline: Instant,
    ) -> Result<String> {
        let started = Instant::now();
        let full_prompt = prompt::build_chatml(
            &prompt::system_prompt(dictionary, smart_formatting),
            transcript,
            smart_formatting,
        );
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

    #[test]
    fn cleans_rambling_request_preserving_names_and_constraints() {
        let gguf = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(
            "Library/Application Support/com.vespry.app/models/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        );
        if !gguf.exists() {
            eprintln!("skipping: cleanup model not downloaded");
            return;
        }
        let mut engine = LlamaCleanup::load(&gguf).expect("loading model");
        let transcript =
            "Looking at the current state of farm code, we have a lot of things going on. \
            I would like for you to go back and do a deeper dive into the project, analyze it, \
            you can use sub threads and I need you to do a cleanup like a code cleanup where \
            if there are any junk, unreachable code, remove it, look for any securities, \
            vulnerabilities, any dead code. I just need some code cleanup, but we need to \
            make sure that we're not breaking any functionalities.";
        let started = Instant::now();
        let cleaned = engine
            .cleanup(
                transcript,
                &["FarmCode".into()],
                false,
                started + Duration::from_secs(30),
            )
            .expect("cleanup");
        println!(
            "FarmCode cleanup ({:.2}s): {cleaned}",
            started.elapsed().as_secs_f32()
        );
        let lower = cleaned.to_lowercase();
        assert!(cleaned.contains("FarmCode"), "name lost: {cleaned}");
        for concept in ["project", "unreachable", "vulnerabilities", "dead code"] {
            assert!(lower.contains(concept), "lost {concept}: {cleaned}");
        }
        assert!(
            lower.contains("subthread") || lower.contains("sub thread"),
            "lost delegation: {cleaned}"
        );
        assert!(
            lower.contains("not breaking")
                || lower.contains("without breaking")
                || lower.contains("don't break")
                || lower.contains("preserv"),
            "lost constraint: {cleaned}"
        );
        assert!(
            cleaned.len() * 10 < transcript.len() * 9,
            "rambling wasn't reduced: {cleaned}"
        );
        assert!(
            prompt::acceptable(transcript, &cleaned),
            "guardrail rejected: {cleaned}"
        );

        assert!(
            engine
                .cleanup(transcript, &[], false, Instant::now())
                .is_err(),
            "an expired request must fall back, not return partial text"
        );
    }

    /// End-to-end cleanup check. Skips (passes) if the GGUF isn't downloaded
    /// yet — run the app once with cleanup enabled first.
    #[test]
    fn cleans_filler_ridden_transcript() {
        let home = std::env::var("HOME").unwrap();
        let gguf = std::path::PathBuf::from(home).join(
            "Library/Application Support/com.vespry.app/models/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        );
        if !gguf.exists() {
            eprintln!("skipping: cleanup model not downloaded");
            return;
        }
        let mut engine = LlamaCleanup::load(&gguf).expect("loading model");
        let transcript =
            "um so basically i think we should uh we should ship the the feature tomorrow you know";
        let cleaned = engine
            .cleanup(
                transcript,
                &[],
                false,
                Instant::now() + Duration::from_secs(30),
            )
            .expect("cleanup");
        println!("cleaned: {cleaned:?}");
        let lower = cleaned.to_lowercase();
        assert!(
            lower.contains("ship the feature tomorrow"),
            "got: {cleaned:?}"
        );
        for filler in ["um", "uh ", "you know", "basically"] {
            assert!(
                !lower.contains(filler),
                "filler {filler:?} survived: {cleaned:?}"
            );
        }
        assert!(
            cleaned.chars().next().unwrap().is_uppercase() && cleaned.ends_with('.'),
            "not sentence-cased: {cleaned:?}"
        );
        assert!(prompt::acceptable(transcript, &cleaned));

        // Regression: clean fragments must pass through, not get "improved"
        // away (observed: "The host without the keystrokes." → "The Host.").
        // Deliberately not one of the few-shot examples.
        let fragment = "the config file without the comments";
        let cleaned = engine
            .cleanup(
                fragment,
                &[],
                false,
                Instant::now() + Duration::from_secs(30),
            )
            .expect("cleanup");
        println!("fragment cleaned: {cleaned:?}");
        assert!(
            cleaned.to_lowercase().contains("without the comments"),
            "content dropped: {cleaned:?}"
        );

        let meaningful = "I like FarmCode, so don't rename it. I think it might need a review.";
        let cleaned = engine
            .cleanup(
                meaningful,
                &[],
                false,
                Instant::now() + Duration::from_secs(30),
            )
            .expect("cleanup");
        println!("meaningful words: {cleaned}");
        for phrase in ["like FarmCode", "don't rename", "I think", "might"] {
            assert!(cleaned.contains(phrase), "lost {phrase}: {cleaned}");
        }
    }

    /// Smart formatting turns a spoken enumeration into a list, and leaves
    /// ordinary prose alone.
    #[test]
    fn formats_enumerations_as_lists() {
        let home = std::env::var("HOME").unwrap();
        let gguf = std::path::PathBuf::from(home).join(
            "Library/Application Support/com.vespry.app/models/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        );
        if !gguf.exists() {
            eprintln!("skipping: cleanup model not downloaded");
            return;
        }
        let mut engine = LlamaCleanup::load(&gguf).expect("loading model");
        let deadline = || Instant::now() + Duration::from_secs(30);

        let spoken =
            "um okay so for the release we need to do a few things first bump the version \
                      then run the tests and finally tag it and push";
        let listed = engine
            .cleanup(spoken, &[], true, deadline())
            .expect("cleanup");
        println!("list output:\n{listed}");
        let list_lines = listed
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with("- ") || t.starts_with("* ") || t[..2.min(t.len())].contains('.')
            })
            .count();
        assert!(list_lines >= 3, "expected a 3-item list, got:\n{listed}");
        assert!(
            prompt::acceptable(spoken, &listed),
            "guardrail rejected the list"
        );

        let unlisted = engine
            .cleanup(spoken, &[], false, deadline())
            .expect("cleanup");
        println!("formatting disabled: {unlisted}");
        assert!(
            !unlisted.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("- ") || line.starts_with("* ") || line.starts_with("1. ")
            }),
            "formatting disabled but got a list: {unlisted}"
        );

        // Prose that merely mentions several things must not become a list.
        let prose = "i grabbed milk eggs and bread on the way home";
        let kept = engine
            .cleanup(prose, &[], true, deadline())
            .expect("cleanup");
        println!("prose output: {kept:?}");
        assert!(
            !kept.contains("\n- ") && !kept.starts_with("- "),
            "prose was bulleted: {kept:?}"
        );
    }
}
