# Vespry

Local, private dictation for your Mac (Windows/Linux planned). Hold a key, speak, release — your words are transcribed on-device, cleaned up by a local LLM, and typed into whatever app you're using. No audio ever leaves your machine.

Inspired by Wispr Flow; architecture informed by the MIT-licensed [Handy](https://github.com/cjpais/Handy).

## Stack

- **Shell:** Tauri v2 · React · TypeScript · Tailwind
- **ASR:** NVIDIA Parakeet TDT 0.6b v3 (ONNX, default) and whisper.cpp (Metal)
- **Cleanup:** Qwen3-1.7B via llama.cpp — removes filler words, fixes punctuation
- **Injection:** clipboard swap + synthetic ⌘V into the focused app

## Development

```
pnpm install
pnpm tauri dev
```

Requires Rust (stable) and Node 20+. Models download on first use into `~/Library/Application Support/Vespry`.
