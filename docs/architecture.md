# Architecture

```
┌──────────────────────────── Melfield (Tauri 2) ────────────────────────────┐
│ UI   React + TypeScript · canvas dot field / orbs · glass components        │
│        ▲ events (spectrum 60 Hz, pulls, jobs)   │ commands                  │
│ Core (crates/melfield-core, Rust, no Tauri dependency)                       │
│   hardware · registry · pull · engines · jobs · audio · store · service      │
│ melfield-api  OpenAI-compatible /v1/audio/speech on 127.0.0.1 (off default)  │
│ melfield-cli  melfield list | pull | say | serve                             │
│        │ framed JSON over stdio                                             │
│ Python workers (workers/), one uv-managed env per engine family             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Principles
- **Voices, not models.** Main screens never show engine names, VRAM or paths. Those live in Settings › Advanced.
- **Zero terminal.** Native built-in voices need no Python; other engines install themselves through a bundled `uv`.
- **Pull, don't bundle.** Weights are downloaded from their original source, pinned and checksummed.
- **Licenses are features.** Every recipe carries license, attribution and usage policy; personal-use models need an explicit accept step.
- **Consent + watermark.** Cloning requires a consent switch; every cloned output is PerTh-watermarked (worker refuses otherwise, core double-checks).
- **Never a stack trace.** Every error is a calm sentence with a one-tap fix. Details go to the log file (Settings › Help › export logs).

## Registry
`registry/recipes/*.json` (one per model) + `registry/voices.json` → `registry/index.json` (built and checked by `scripts/build_registry.py`). The index is compiled into the app; a signed remote copy can refresh it without an app update once a signing key is published.

A recipe pins `repo@revision`, every file's size (and sha256 for large files), the engine (`native` or `python` family), languages, capabilities, hardware requirements and license.

## Downloads
Resumable HTTP range requests, sha256 verification, atomic install, disk-space checks that include the one-time engine setup. Files land in a Hugging Face–style cache (`models/hf/models--org--name/snapshots/<rev>/…`) so Python engines can load them with `HF_HUB_OFFLINE=1`.

## Engines
- **Native:** Kokoro-82M (int8 ONNX) via sherpa-onnx. English text is phonemized by misaki-rs (MIT) plus Melfield's out-of-vocabulary fallback — espeak-ng (GPL) is never linked.
- **Python:** one supervised worker process per family (Qwen3, Chatterbox, Pocket, Parler, VoxCPM). Lazy start, restart once on crash, "repair" rebuilds the env, idle workers shut down after 10 minutes. See [protocol.md](protocol.md).

## Hardware tiers
| Tier | Detected | Python engines run on |
|---|---|---|
| A | NVIDIA ≥ 8 GB | CUDA (everything) |
| A− | NVIDIA 4–8 GB | CUDA (smaller models) |
| B | Apple Silicon, macOS 14+ | MPS |
| B− | Apple Silicon, macOS < 14 | built-in voices only |
| C | AMD / Intel GPU | CPU |
| D | CPU only | CPU |
| E | Intel Mac | built-in voices only (no PyTorch builds for Intel Macs) |

Voices that can't run on a machine are hidden; voices that would run far below real time are marked "slow on this computer".

## Cloning engine choice (automatic)
1. Indian languages → Chatterbox Multilingual v3 (or VoxCPM2 on big GPUs).
2. GPU / Apple Silicon → Qwen3-TTS (1.7B with ≥ 8 GB, else 0.6B).
3. No GPU, English → Pocket TTS.
4. Otherwise → Chatterbox Multilingual v3.

## Audio
Playback and recording are native (cpal), not in the webview, so they behave the same on every OS. Long text is chunked on sentence boundaries (incl. Devanagari `।`), synthesized chunk by chunk, crossfaded and loudness-normalised (EBU R128, −16 LUFS); the first chunk plays while the rest render. A 64-band spectrum drives the dot field in real time.
