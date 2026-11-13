"""Watermark every cloned output with Resemble's PerTh implicit watermarker (MIT).

Rule: a clone request must never return unwatermarked audio.
PerTh's package sets PerthImplicitWatermarker = None when its import fails,
so we check explicitly and refuse instead of silently skipping.
"""

from __future__ import annotations

import numpy as np

from .errors import WorkerError

_watermarker = None


def _get():
    global _watermarker
    if _watermarker is None:
        try:
            import perth  # type: ignore[import-not-found]
        except Exception as exc:  # noqa: BLE001 — any import failure means "unavailable"
            raise WorkerError("WATERMARK_UNAVAILABLE", f"perth import failed: {exc}") from exc
        cls = getattr(perth, "PerthImplicitWatermarker", None)
        if cls is None:
            raise WorkerError("WATERMARK_UNAVAILABLE", "perth.PerthImplicitWatermarker is None")
        _watermarker = cls()
    return _watermarker


def available() -> bool:
    try:
        _get()
        return True
    except WorkerError:
        return False


def apply(samples: np.ndarray, sample_rate: int) -> np.ndarray:
    wm = _get()
    out = wm.apply_watermark(np.asarray(samples, dtype=np.float32), sample_rate=sample_rate)
    return np.asarray(out, dtype=np.float32)


def detect(samples: np.ndarray, sample_rate: int) -> bool:
    wm = _get()
    return float(wm.get_watermark(np.asarray(samples, dtype=np.float32), sample_rate=sample_rate)) > 0.5
