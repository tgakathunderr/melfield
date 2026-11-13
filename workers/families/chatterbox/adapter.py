"""Chatterbox Multilingual v3 + Turbo (MIT, Resemble AI). Both apply the PerTh
watermark inside `generate()`, so `builtin_watermark = True`; tests verify it
with PerTh's detector."""

from __future__ import annotations

import inspect

from melfield_worker.adapter import LoadRequest, SynthRequest, to_mono_f32, torch_device
from melfield_worker.errors import WorkerError

EXAGGERATION = {"calm": 0.3, "natural": 0.5, "expressive": 0.8}
SUPPORTED = {"ar", "da", "de", "el", "en", "es", "fi", "fr", "he", "hi", "it", "ja", "ko", "ms", "nl", "no", "pl", "pt", "ru", "sv", "sw", "tr", "zh"}


class ChatterboxAdapter:
    builtin_watermark = True

    def __init__(self) -> None:
        self.model = None
        self.entry = "multilingual_v3"

    def load(self, req: LoadRequest) -> None:
        import perth  # noqa: F401 — fail early (WATERMARK_UNAVAILABLE path) if missing

        if getattr(perth, "PerthImplicitWatermarker", None) is None:
            raise WorkerError("WATERMARK_UNAVAILABLE", "perth implicit watermarker missing")
        device = torch_device(req.device)
        ckpt = req.artifact_dir(0)
        self.entry = req.entry
        if req.entry == "turbo":
            from chatterbox.tts_turbo import ChatterboxTurboTTS  # type: ignore[import-not-found]

            self.model = ChatterboxTurboTTS.from_local(ckpt, device)
        else:
            from chatterbox.mtl_tts import ChatterboxMultilingualTTS  # type: ignore[import-not-found]

            params = inspect.signature(ChatterboxMultilingualTTS.from_local).parameters
            kwargs = {"t3_model": "v3"} if "t3_model" in params else {}
            self.model = ChatterboxMultilingualTTS.from_local(ckpt, device, **kwargs)

    def synthesize(self, req: SynthRequest, emit, cancelled) -> None:
        if self.model is None:
            raise WorkerError("MODEL_FILES_MISSING", "not loaded")
        lang = req.lang.split("-")[0]
        if cancelled():
            return
        if self.entry == "turbo":
            if lang != "en":
                raise WorkerError("UNSUPPORTED_LANGUAGE", lang)
            wav = self.model.generate(req.text, audio_prompt_path=req.ref_audio)
        else:
            if lang not in SUPPORTED:
                raise WorkerError("UNSUPPORTED_LANGUAGE", lang)
            wav = self.model.generate(
                req.text,
                language_id=lang,
                audio_prompt_path=req.ref_audio,
                exaggeration=EXAGGERATION.get(req.delivery, 0.5),
                cfg_weight=0.5,
            )
        emit(to_mono_f32(wav), int(self.model.sr))


def create() -> ChatterboxAdapter:
    return ChatterboxAdapter()
