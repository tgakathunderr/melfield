"""Test-only family: deterministic tones, no ML dependencies. Used by protocol tests."""

from __future__ import annotations

import numpy as np

from melfield_worker.adapter import LoadRequest, SynthRequest
from melfield_worker.errors import WorkerError


class FakeAdapter:
    builtin_watermark = False

    def __init__(self) -> None:
        self.rate = 24000
        self.loaded = False

    def load(self, req: LoadRequest) -> None:
        if req.entry == "missing":
            raise FileNotFoundError("model.safetensors not found")
        self.rate = req.sample_rate
        self.loaded = True

    def synthesize(self, req: SynthRequest, emit, cancelled) -> None:
        if not self.loaded:
            raise WorkerError("MODEL_FILES_MISSING", "load first")
        if req.lang not in ("en", "hi"):
            raise WorkerError("UNSUPPORTED_LANGUAGE", req.lang)
        if req.text == "__oom__":
            raise MemoryError("CUDA out of memory")
        seconds = max(0.2, len(req.text) / 15.0) / max(0.5, req.speed)
        n = int(seconds * self.rate)
        t = np.arange(n, dtype=np.float32) / self.rate
        audio = (0.3 * np.sin(2 * np.pi * 220.0 * t)).astype(np.float32)
        for i in range(0, n, self.rate // 4):
            if cancelled():
                return
            emit(audio[i : i + self.rate // 4], self.rate)


def create() -> FakeAdapter:
    return FakeAdapter()
