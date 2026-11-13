"""Qwen3-TTS (Apache-2.0) via `qwen-tts`. Base = cloning; CustomVoice = 9 built-in speakers.

No streaming in the qwen-tts package: we return whole chunks;
the core streams sentence by sentence.
"""

from __future__ import annotations

from melfield_worker.adapter import ISO_TO_NAME, LoadRequest, SynthRequest, to_mono_f32, torch_device
from melfield_worker.errors import WorkerError

DELIVERY_INSTRUCT = {
    "calm": "Speak calmly and gently, at an unhurried pace.",
    "natural": "",
    "expressive": "Speak expressively, with lively, varied intonation.",
}


class QwenAdapter:
    builtin_watermark = False

    def __init__(self) -> None:
        self.model = None
        self.entry = "base"

    def load(self, req: LoadRequest) -> None:
        import torch
        from qwen_tts import Qwen3TTSModel  # type: ignore[import-not-found]

        device = torch_device(req.device)
        dtype = torch.bfloat16 if device == "cuda" else torch.float32
        self.model = Qwen3TTSModel.from_pretrained(
            req.artifact_dir(0),
            device_map=f"{device}:0" if device == "cuda" else device,
            dtype=dtype,
            attn_implementation="sdpa",  # flash-attn is optional and absent from our envs
        )
        self.entry = req.entry

    def synthesize(self, req: SynthRequest, emit, cancelled) -> None:
        if self.model is None:
            raise WorkerError("MODEL_FILES_MISSING", "not loaded")
        language = ISO_TO_NAME.get(req.lang.split("-")[0])
        if language is None or language == "Hindi":
            raise WorkerError("UNSUPPORTED_LANGUAGE", req.lang)
        if cancelled():
            return
        if self.entry == "custom_voice":
            instruct = DELIVERY_INSTRUCT.get(req.delivery, "")
            wavs, sr = self.model.generate_custom_voice(text=req.text, language=language, speaker=req.preset or "Ryan", instruct=instruct)
        else:
            if not req.ref_audio:
                raise WorkerError("BAD_REFERENCE_AUDIO", "base model needs a reference")
            kwargs = {"ref_audio": req.ref_audio}
            if req.ref_text:
                kwargs["ref_text"] = req.ref_text
            else:
                kwargs["x_vector_only_mode"] = True  # lower quality, but works without a transcript
            wavs, sr = self.model.generate_voice_clone(text=req.text, language=language, **kwargs)
        emit(to_mono_f32(wavs), int(sr))


def create() -> QwenAdapter:
    return QwenAdapter()
