"""Adapter contract every engine family implements."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, Iterator, Protocol

import numpy as np

ISO_TO_NAME = {
    "zh": "Chinese", "en": "English", "ja": "Japanese", "ko": "Korean", "de": "German", "fr": "French",
    "ru": "Russian", "pt": "Portuguese", "es": "Spanish", "it": "Italian", "hi": "Hindi",
}


@dataclass(frozen=True)
class LoadRequest:
    recipe: str
    entry: str
    device: str
    sample_rate: int
    artifacts: list[dict[str, Any]] = field(default_factory=list)

    def artifact_dir(self, index: int = 0) -> str:
        return str(self.artifacts[index]["dir"])


@dataclass(frozen=True)
class SynthRequest:
    text: str
    lang: str
    preset: str | None
    ref_audio: str | None
    ref_text: str | None
    speed: float
    delivery: str
    watermark: bool


ChunkSink = Callable[[np.ndarray, int], None]
Cancelled = Callable[[], bool]


class Adapter(Protocol):
    #: True when the model itself watermarks clone output (e.g. Chatterbox/PerTh).
    builtin_watermark: bool

    def load(self, req: LoadRequest) -> None: ...

    def synthesize(self, req: SynthRequest, emit: ChunkSink, cancelled: Cancelled) -> None: ...


def torch_device(device: str) -> str:
    """Map our device names to torch's, falling back to CPU when unavailable."""
    import torch

    if device == "cuda" and torch.cuda.is_available():
        return "cuda"
    if device == "mps" and getattr(torch.backends, "mps", None) and torch.backends.mps.is_available():
        return "mps"
    return "cpu"


def to_mono_f32(audio: Any) -> np.ndarray:
    """Accept torch tensors / numpy arrays / lists of either; return 1-D float32."""
    if isinstance(audio, (list, tuple)):
        audio = audio[0] if len(audio) == 1 else np.concatenate([to_mono_f32(a) for a in audio])
    if hasattr(audio, "detach"):
        audio = audio.detach().float().cpu().numpy()
    arr = np.asarray(audio, dtype=np.float32)
    if arr.ndim > 1:
        arr = arr.reshape(-1) if 1 in arr.shape else arr.mean(axis=0)
    return np.nan_to_num(arr, copy=False)


def chunked(samples: np.ndarray, size: int) -> Iterator[np.ndarray]:
    for i in range(0, len(samples), size):
        yield samples[i : i + size]
