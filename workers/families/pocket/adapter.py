"""Kyutai Pocket TTS (code MIT, weights CC-BY-4.0). CPU-first, streams natively.

The library resolves `hf://` weight paths through huggingface_hub; the core pulls
those exact files into HF_HUB_CACHE and runs us with HF_HUB_OFFLINE=1. Cloning
weights come from the gated repo (pulled only after the user signs in + agrees).
"""

from __future__ import annotations

from melfield_worker.adapter import LoadRequest, SynthRequest, to_mono_f32
from melfield_worker.errors import WorkerError


class PocketAdapter:
    builtin_watermark = False

    def __init__(self) -> None:
        self.model = None
        self.voices: dict[str, object] = {}

    def load(self, req: LoadRequest) -> None:
        from pocket_tts import TTSModel  # type: ignore[import-not-found]

        self.model = TTSModel.load_model()
        self.voices.clear()

    def _voice_state(self, req: SynthRequest):
        key = req.ref_audio or req.preset or "alba"
        if key not in self.voices:
            if len(self.voices) > 8:
                self.voices.clear()
            try:
                self.voices[key] = self.model.get_state_for_audio_prompt(key)
            except Exception as exc:  # noqa: BLE001
                if req.ref_audio:
                    raise WorkerError("BAD_REFERENCE_AUDIO", str(exc)) from exc
                raise
        return self.voices[key]

    def synthesize(self, req: SynthRequest, emit, cancelled) -> None:
        if self.model is None:
            raise WorkerError("MODEL_FILES_MISSING", "not loaded")
        if req.lang.split("-")[0] != "en":
            raise WorkerError("UNSUPPORTED_LANGUAGE", req.lang)
        state = self._voice_state(req)
        rate = int(self.model.sample_rate)
        stream = getattr(self.model, "generate_audio_stream", None)
        if stream is not None:
            for chunk in stream(state, req.text):
                if cancelled():
                    return
                emit(to_mono_f32(chunk), rate)
        else:
            emit(to_mono_f32(self.model.generate_audio(state, req.text)), rate)


def create() -> PocketAdapter:
    return PocketAdapter()
