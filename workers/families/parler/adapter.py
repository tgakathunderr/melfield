"""Indic Parler-TTS (Apache-2.0, AI4Bharat). Voices are described in words;
named speakers ("Divya", "Rohit"…) come from the model card's list. The
description tokenizer (flan-t5-large) is pulled as the recipe's second artifact."""

from __future__ import annotations

from melfield_worker.adapter import LoadRequest, SynthRequest, to_mono_f32, torch_device
from melfield_worker.errors import WorkerError

SUPPORTED = {"as", "bn", "brx", "doi", "en", "gu", "hi", "kn", "kok", "mai", "ml", "mni", "mr", "ne", "or", "sa", "sat", "sd", "ta", "te", "ur"}
DELIVERY = {
    "calm": "speaks slowly and calmly in a soft tone",
    "natural": "speaks at a moderate pace in a warm, natural tone",
    "expressive": "speaks expressively with an animated, lively tone",
}


def describe(preset: str | None, delivery: str) -> str:
    who = f"{preset}'s voice" if preset else "A clear voice"
    return f"{who} {DELIVERY.get(delivery, DELIVERY['natural'])}, with very clear audio and no background noise."


class ParlerAdapter:
    builtin_watermark = False

    def __init__(self) -> None:
        self.model = None
        self.tokenizer = None
        self.desc_tokenizer = None
        self.device = "cpu"

    def load(self, req: LoadRequest) -> None:
        from parler_tts import ParlerTTSForConditionalGeneration  # type: ignore[import-not-found]
        from transformers import AutoTokenizer

        self.device = torch_device(req.device)
        self.model = ParlerTTSForConditionalGeneration.from_pretrained(req.artifact_dir(0)).to(self.device)
        self.tokenizer = AutoTokenizer.from_pretrained(req.artifact_dir(0))
        self.desc_tokenizer = AutoTokenizer.from_pretrained(req.artifact_dir(1))

    def synthesize(self, req: SynthRequest, emit, cancelled) -> None:
        import torch

        if self.model is None:
            raise WorkerError("MODEL_FILES_MISSING", "not loaded")
        if req.lang.split("-")[0] not in SUPPORTED:
            raise WorkerError("UNSUPPORTED_LANGUAGE", req.lang)
        if cancelled():
            return
        desc = self.desc_tokenizer(describe(req.preset, req.delivery), return_tensors="pt").to(self.device)
        prompt = self.tokenizer(req.text, return_tensors="pt").to(self.device)
        with torch.inference_mode():
            gen = self.model.generate(
                input_ids=desc.input_ids,
                attention_mask=desc.attention_mask,
                prompt_input_ids=prompt.input_ids,
                prompt_attention_mask=prompt.attention_mask,
            )
        emit(to_mono_f32(gen), int(self.model.config.sampling_rate))


def create() -> ParlerAdapter:
    return ParlerAdapter()
