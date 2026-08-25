# Vespry — notes for Claude

Wispr Flow-style local dictation app. Tauri v2 + React/TS/Tailwind front, Rust core. Mac-first; Windows/Linux ports planned (that's why Tauri over Swift).

## Architecture (src-tauri/src)

- `controller.rs` — the pipeline state machine on its own thread (owns the !Send cpal stream and the loaded models). Events in via mpsc: HotkeyPressed/Released, Toggle (tray), PreloadModel, PreloadCleanup. States out via `dictation-state` Tauri events: idle → listening → transcribing → cleaning → injecting.
- `shortcuts.rs` — rdev (rustdesk fork) global listener; hotkey id read from settings per-event. Hold = push-to-talk; tap <300 ms arms hands-free.
- `audio.rs` — cpal capture at device rate → rubato resample to 16 kHz mono on stop; emits `audio-level` RMS events ~20 Hz for the HUD.
- `asr/` — `Transcriber` trait; `parakeet.rs` (transcribe-rs ONNX int8, default), `whisper.rs` (transcribe-rs whisper-metal). Controller reloads when settings' engine/model/language key changes.
- `cleanup/` — `llama.rs` (llama-cpp-2, Metal, greedy, fresh context per call) + `prompt.rs` (few-shot ChatML, Qwen3 empty-think prefill, postprocess + length-ratio guardrail). Cleanup failure always falls back to raw transcript.
- `inject.rs` — clipboard save → set → enigo ⌘V → restore after 350 ms.
- `hud.rs` — the pill. macOS: config-defined hidden "hud" window converted via tauri-nspanel `to_panel()` **after launch, on the main thread** (PanelBuilder inside setup() throws an objc exception → abort). Non-activating style mask, all-Spaces + fullscreen-auxiliary.
- `models.rs` — downloads to `~/Library/Application Support/com.vespry.app/models` with `.part` staging; `model-download` progress events.
- `settings.rs` / `history.rs` — plain JSON files in app data dir.

## Gotchas

- Bash cwd: cargo commands must run from `src-tauri/`.
- `cargo add` failures can be silent when output is piped — verify Cargo.toml after.
- Tests are headless: `say` synthesizes speech → both ASR engines + cleanup e2e. They skip (pass) when models aren't downloaded. `cd src-tauri && cargo test --lib`.
- In `tauri dev`, TCC attributes mic/accessibility to the parent terminal; use the built .app for permission-accurate testing.
- whisper-metal + llama-cpp-2 need cmake; first build is slow.
- Commits: user's global rules — no co-author trailers, subject + why body.
