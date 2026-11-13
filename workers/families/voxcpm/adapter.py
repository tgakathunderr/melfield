"""VoxCPM2 (Apache-2.0, OpenBMB): 48 kHz, 30 languages incl. Hindi, cloning.
Big-GPU only (≈8 GB VRAM). Usage policy forbids impersonation/fraud — enforced by
Melfield's consent step and watermark."""

from __future__ import annotations

import inspect

from melfield_worker.adapter import LoadRequest, SynthRequest, to_mono_f32, torch_device
from melfield_worker.errors import WorkerError


class VoxCpmAdapter:
    builtin_watermark = False

    def __init__(self) -> None:
        self.model = None
        self.rate = 48000

    def load(self, req: LoadRequest) -> None:
        from voxcpm import VoxCPM  # type: ignore[import-not-found]

        device = torch_device(req.device)
        params = inspect.signature(VoxCPM.from_pretrained).parameters
        kwargs = {"load_denoiser": False} if "load_denoiser" in params else {}
        if "device" in params:
            kwargs["device"] = device
        self.model = VoxCPM.from_pretrained(req.artifact_dir(0), **kwargs)
        tts = getattr(self.model, "tts_model", None)
        self.rate = int(getattr(tts, "sample_rate", req.sample_rate))

    def synthesize(self, req: SynthRequest, emit, cancelled) -> None:
        if self.model is None:
            raise WorkerError("MODEL_FILES_MISSING", "not loaded")
        if not req.ref_audio:
            raise WorkerError("BAD_REFERENCE_AUDIO", "voxcpm voices in melfield are clones")
        kwargs = {"text": req.text, "prompt_wav_path": req.ref_audio}
        if req.ref_text:
            kwargs["prompt_text"] = req.ref_text
        stream = getattr(self.model, "generate_streaming", None)
        if stream is not None:
            for chunk in stream(**kwargs):
                if cancelled():
                    return
                emit(to_mono_f32(chunk), self.rate)
        else:
            emit(to_mono_f32(self.model.generate(**kwargs)), self.rate)


def create() -> VoxCpmAdapter:
    return VoxCpmAdapter()
