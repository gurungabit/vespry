# Vespry

[![CI](https://github.com/gurungabit/vespry/actions/workflows/ci.yml/badge.svg)](https://github.com/gurungabit/vespry/actions/workflows/ci.yml)
[![Release](https://github.com/gurungabit/vespry/actions/workflows/release.yml/badge.svg)](https://github.com/gurungabit/vespry/releases)

Local, private dictation for your Mac (Windows/Linux planned). Hold a key, speak, release — your words are transcribed on-device, cleaned up by a local LLM, and typed into whatever app you're using. No audio ever leaves your machine.

Inspired by Wispr Flow; architecture informed by the MIT-licensed [Handy](https://github.com/cjpais/Handy).

## Features

- **Hold right ⌘** (configurable: right ⌥, Fn, left ⌃, F5) to talk; release to insert at the cursor. Quick-tap for hands-free mode — tap again to finish. Also triggerable from the menu-bar tray.
- **ASR engines:** NVIDIA Parakeet TDT 0.6b v3 (ONNX, default — sub-second, 25 European languages) or whisper.cpp (Metal — base/small/large-v3-turbo, ~100 languages with a language hint).
- **AI cleanup:** Qwen3-4B-Instruct (llama.cpp, Metal) removes filler words and false starts, fixes punctuation, and applies your custom dictionary. Guardrailed: any failure or over-rewrite falls back to the raw transcript.
- **Wispr-style HUD:** a non-activating floating pill with a live waveform — never steals focus, shows on every Space including fullscreen apps.
- **History** (last 500 dictations, raw + cleaned), custom dictionary, chimes, launch at login.

## Requirements

- macOS 14+ on Apple Silicon (developed on macOS 26)
- Permissions: Microphone + Accessibility (for the global hotkey and text insertion)
- Models download on first run into `~/Library/Application Support/com.vespry.app/models` (~640 MB Parakeet + ~2.4 GB Qwen3; whisper models on demand)

## Development

```
pnpm install
pnpm tauri dev
```

Requires Rust (stable), Node 20+, and cmake (`brew install cmake`). Note: in `tauri dev` the binary is unbundled, so macOS attributes mic/accessibility permission prompts to your terminal app — for permission-accurate testing use the release bundle:

```
pnpm tauri build
open src-tauri/target/release/bundle/macos/Vespry.app
```

Model downloads use `https://huggingface.co` by default. On networks that require an internal Hugging Face mirror, set its endpoint when running or building the app (the mirror must preserve Hugging Face's `<repo>/resolve/main/<file>` URL layout). Authenticated mirrors can receive a bearer token through `VESPRY_HF_TOKEN` (or the standard `HF_TOKEN`):

```
VESPRY_HF_ENDPOINT=https://your-internal-hugging-face-endpoint \
  VESPRY_HF_TOKEN="$YOUR_INTERNAL_TOKEN" \
  pnpm tauri dev
```

The build captures the endpoint when it is set during `pnpm tauri build`. Tokens are read only at runtime and are never embedded in the application binary.

The endpoint can also be set in Vespry under **Settings → Models → Model download endpoint**. For example:

```
https://huggingface-mirror.example.com
```

Tests (headless — synthesize speech with `say`, run it through both ASR engines and the cleanup LLM):

```
cd src-tauri && cargo test --lib
```

## Releases

Grab the latest DMG from [Releases](https://github.com/gurungabit/vespry/releases), drag Vespry to Applications, then run this once before opening it:

```
xattr -dr com.apple.quarantine /Applications/Vespry.app
```

Builds are ad-hoc signed but not notarized (that needs a paid Apple Developer account), so macOS quarantines the download and claims the app is **"damaged"** until that flag is cleared. It isn't — and on Apple Silicon, right-click → Open does *not* bypass this; the `xattr` command is the fix.

**Which build am I running?** Settings → About shows the version and the exact commit it was built from.

To cut a release:

```
scripts/release.sh 0.2.0
```

That bumps the version in `package.json`, `tauri.conf.json`, and `Cargo.toml`, commits, tags `v0.2.0`, and pushes — the Release workflow then builds the app on a macOS runner and publishes the DMG to GitHub Releases automatically.
