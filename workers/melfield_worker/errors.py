"""Worker error codes. The Rust core maps these to calm human messages;
details only ever reach the log file."""

from __future__ import annotations


class WorkerError(Exception):
    def __init__(self, code: str, detail: str = "") -> None:
        super().__init__(f"{code}: {detail}")
        self.code = code
        self.detail = detail


def classify(exc: BaseException) -> WorkerError:
    """Turn any exception into a WorkerError with a stable code."""
    if isinstance(exc, WorkerError):
        return exc
    name = type(exc).__name__
    text = str(exc)
    lowered = text.lower()
    if "OutOfMemory" in name or "out of memory" in lowered or "mps backend out of memory" in lowered:
        return WorkerError("OUT_OF_MEMORY", f"{name}: {text}")
    if isinstance(exc, (ImportError, ModuleNotFoundError)):
        return WorkerError("DEPENDENCY_MISSING", f"{name}: {text}")
    if isinstance(exc, FileNotFoundError) or "local_files_only" in text or "offline mode" in lowered:
        return WorkerError("MODEL_FILES_MISSING", f"{name}: {text}")
    if "cuda" in lowered and ("not available" in lowered or "no kernel image" in lowered or "driver" in lowered):
        return WorkerError("DEVICE_UNAVAILABLE", f"{name}: {text}")
    return WorkerError("INTERNAL", f"{name}: {text}")
