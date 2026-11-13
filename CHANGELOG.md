# Changelog

## 0.1.0 — 2026-11-13

First public release.

### Added
- Desktop app (Tauri 2 + React): first launch with dots-fill download, Studio, Voices, Clone, History, Settings — matching the approved design language.
- Built-in voices with no Python: Kokoro-82M via sherpa-onnx (int8, 142 MB starter download), six English voices (Aria, Kabir, Mira, Theo, Nova, George) with instant hover previews.
- GPL-free English pronunciation: misaki-rs plus Melfield's out-of-vocabulary fallback (names, brands, acronyms), time/month/percent normalisation.
- Unlimited text: sentence chunking (incl. Devanagari danda), streaming playback of the first chunk, equal-power stitching, EBU R128 loudness levelling.
- Dot field reacts to the real playback spectrum (64 bands computed natively, 60 Hz).
- Pull system: signed-registry-ready recipes pinned to commit SHAs, resumable sha256-verified downloads into a Hugging Face–style cache, disk-space checks, license badges, personal-use accept step, Hugging Face sign-in (OAuth when configured; read-key fallback).
- Python engine workers (uv-managed, one env per family) with supervisor, crash restart, repair, and adapters for Qwen3-TTS, Chatterbox Multilingual v3 + Turbo, Pocket TTS, Indic Parler-TTS and VoxCPM2.
- Cloning: native mic recording, file import, reference cleanup, read-aloud prompt (transcript), consent switch, automatic engine choice, PerTh watermark on every clone output, "dots converge" reveal.
- History (replay, re-download, reopen, favourite, delete, search) and Settings (default voice, save format, compute, calmer visuals, storage usage + move, updates, logs export, Advanced: local API, engines, repair, credits).
- Local OpenAI-compatible API (`/v1/audio/speech`, loopback only, off by default) and `melfield` CLI (`list`, `pull`, `say`, `serve`).
- CI on Windows, macOS (arm64 + x64) and Linux; release workflow for signed installers and updater manifest.

### Fixed before release
- Opening the voice picker in Studio crashed the app.
- Links and "open folder" buttons did nothing (opener permissions); they now go through the app core.
- Hugging Face sign-in is a clear three-step guide that detects access by itself.
- Saving a clip confirms with "show in folder"; real drag-and-drop on the Clone screen.

### Known gaps
See README › Status for what is verified and what is still in progress.
