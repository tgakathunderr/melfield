# Third-party notices

Melfield's own code is MIT. It bundles or downloads the following. Model weights are **not** bundled — they are downloaded on demand from their original source under their own licenses.

## Bundled with the app
| Component | License | Notes |
|---|---|---|
| Tauri 2 and plugins | MIT / Apache-2.0 | App shell |
| sherpa-onnx | Apache-2.0 | Native TTS runtime (GPL-free since 2.0) |
| ONNX Runtime (via sherpa-onnx) | MIT | |
| misaki-rs | MIT | English G2P; lexicon data from misaki (Apache-2.0). Built without the optional espeak feature |
| uv (Astral) | MIT OR Apache-2.0 | Bundled helper that installs Python engines |
| cpal, hound, rubato, rustfft, symphonia, flacenc, ebur128 | Apache-2.0 / MIT / MPL-2.0 (symphonia) | Audio I/O, DSP, codecs |
| rusqlite (SQLite) | MIT (SQLite: public domain) | |
| reqwest, rustls, tokio, axum, serde | MIT / Apache-2.0 | |
| React, zustand | MIT | UI |
| Inter, Inter Tight, JetBrains Mono | SIL Open Font License 1.1 | Bundled fonts |

A full machine-generated list of Rust and npm dependencies with license texts is produced for each release (`cargo about` / `license-checker`) and attached to the GitHub release.

## Downloaded on demand (voices)
| Voice engine | Weights license | Attribution |
|---|---|---|
| Kokoro-82M (hexgrad), ONNX by k2-fsa | Apache-2.0 | Training data includes CC BY corpora (Koniwa, SIWIS) |
| Qwen3-TTS (Alibaba Qwen) | Apache-2.0 | |
| Chatterbox Multilingual v3 / Turbo (Resemble AI) | MIT | Outputs carry a PerTh watermark |
| Pocket TTS (Kyutai) | **CC BY 4.0** (code MIT) | "Pocket TTS by Kyutai" — voice embeddings per kyutai/tts-voices |
| Indic Parler-TTS (AI4Bharat, Hugging Face) | Apache-2.0 | Gated: requires the user's own Hugging Face acceptance |
| flan-t5-large tokenizer (Google) | Apache-2.0 | Used by Indic Parler-TTS |
| VoxCPM2 (OpenBMB) | Apache-2.0 | Authors forbid impersonation, fraud and disinformation |

## Python engine dependencies (installed on demand)
PyTorch (BSD-3-Clause), transformers (Apache-2.0), qwen-tts (Apache-2.0), chatterbox-tts (MIT), resemble-perth (MIT), pocket-tts (MIT), parler-tts (Apache-2.0), voxcpm (Apache-2.0), and their dependencies under their respective licenses.

## Not used
- **espeak-ng (GPL-3.0)** is deliberately not bundled or linked (the GPL would extend to the whole app).
- **LAME (LGPL)** is not shipped yet; MP3 export is not available yet.
