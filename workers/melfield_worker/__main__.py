"""Worker entry point: `python -m melfield_worker --family <name>`.

stdout carries protocol frames only. Anything libraries print goes to stderr
(the engine log file), so a stray print can never corrupt the channel.
"""

from __future__ import annotations

import argparse
import importlib
import logging
import os
import queue
import sys
import threading
import traceback
from typing import Any

import numpy as np

from . import __version__, watermark
from .adapter import Adapter, LoadRequest, SynthRequest
from .errors import WorkerError, classify
from .protocol import PROTOCOL_VERSION, Channel

log = logging.getLogger("melfield.worker")


def _error(req_id: Any, err: WorkerError) -> dict[str, Any]:
    return {"id": req_id, "ok": False, "error": {"code": err.code, "detail": err.detail[:2000]}}


class Worker:
    def __init__(self, family: str, channel: Channel) -> None:
        self.family = family
        self.channel = channel
        self.adapter: Adapter | None = None
        self.loaded: str | None = None
        self.cancel = threading.Event()
        self.inbox: "queue.Queue[dict[str, Any] | None]" = queue.Queue()

    def _adapter(self) -> Adapter:
        if self.adapter is None:
            module = importlib.import_module(f"families.{self.family}.adapter")
            self.adapter = module.create()
        return self.adapter

    def reader(self) -> None:
        """Read frames on a thread so `cancel` is seen mid-synthesis."""
        while True:
            try:
                msg = self.channel.recv()
            except Exception:  # noqa: BLE001 — EOF or garbage both mean "parent is gone"
                self.inbox.put(None)
                return
            if msg.get("op") == "cancel":
                self.cancel.set()
                continue
            self.inbox.put(msg)

    def handle(self, msg: dict[str, Any]) -> bool:
        op, req_id = msg.get("op"), msg.get("id")
        if op == "hello":
            self.channel.send({
                "id": req_id, "ok": True, "family": self.family, "version": __version__,
                "protocol": PROTOCOL_VERSION, "watermark": "perth" if watermark.available() else None,
            })
        elif op == "health":
            self.channel.send({"id": req_id, "ok": True, "loaded": self.loaded})
        elif op == "load":
            self.channel.send({"id": req_id, "event": "progress", "stage": "loading", "pct": 10})
            req = LoadRequest(
                recipe=msg["recipe"], entry=msg["entry"], device=msg.get("device", "cpu"),
                sample_rate=int(msg.get("sample_rate", 24000)), artifacts=list(msg.get("artifacts", [])),
            )
            self._adapter().load(req)
            self.loaded = req.recipe
            self.channel.send({"id": req_id, "ok": True})
        elif op == "synthesize":
            self.synthesize(req_id, msg)
        elif op == "shutdown":
            return False
        else:
            raise WorkerError("INTERNAL", f"unknown op {op!r}")
        return True

    def synthesize(self, req_id: Any, msg: dict[str, Any]) -> None:
        voice = msg.get("voice") or {}
        req = SynthRequest(
            text=str(msg.get("text", "")), lang=str(msg.get("lang", "en")), preset=voice.get("preset"),
            ref_audio=voice.get("ref_audio"), ref_text=voice.get("ref_text"), speed=float(msg.get("speed", 1.0)),
            delivery=str(msg.get("delivery", "natural")), watermark=bool(msg.get("watermark", False)),
        )
        adapter = self._adapter()
        needs_mark = req.watermark and not adapter.builtin_watermark
        if req.watermark:
            watermark._get()  # refuse up front when the watermarker is missing
        self.cancel.clear()
        seq = 0
        # Watermark needs whole-utterance context; buffer when marking, stream otherwise.
        buffered: list[np.ndarray] = []
        rate_seen = 0

        def emit(samples: np.ndarray, sample_rate: int) -> None:
            nonlocal seq, rate_seen
            rate_seen = sample_rate
            if needs_mark:
                buffered.append(samples)
                return
            self.channel.send(
                {"id": req_id, "event": "chunk", "seq": seq, "sample_rate": sample_rate, "format": "f32le"},
                np.asarray(samples, dtype="<f4").tobytes(),
            )
            seq += 1

        adapter.synthesize(req, emit, self.cancel.is_set)
        if self.cancel.is_set():
            raise WorkerError("CANCELLED", "cancelled by parent")
        if needs_mark and buffered:
            marked = watermark.apply(np.concatenate(buffered), rate_seen)
            self.channel.send(
                {"id": req_id, "event": "chunk", "seq": 0, "sample_rate": rate_seen, "format": "f32le"},
                marked.astype("<f4").tobytes(),
            )
        self.channel.send({"id": req_id, "ok": True, "watermarked": bool(req.watermark)})

    def run(self) -> None:
        threading.Thread(target=self.reader, name="reader", daemon=True).start()
        while True:
            msg = self.inbox.get()
            if msg is None:
                return
            try:
                if not self.handle(msg):
                    return
            except BaseException as exc:  # noqa: BLE001 — every failure becomes a coded reply
                err = classify(exc)
                log.error("op %s failed: %s\n%s", msg.get("op"), err.detail, traceback.format_exc())
                self.channel.send(_error(msg.get("id"), err))


def _isolate_stdio():
    """Move the protocol onto private duplicates of fd 0/1 and point the real
    std handles at NUL (stdin) and stderr (stdout).

    Why: on Windows, a thread blocked reading a pipe makes other calls on that
    same handle (e.g. GetFileType from native libraries loading BLAS) block too —
    a deadlock during model load. It also guarantees C-level prints can never
    corrupt protocol frames.
    """
    in_fd, out_fd = os.dup(0), os.dup(1)
    if sys.platform == "win32":
        import msvcrt

        msvcrt.setmode(in_fd, os.O_BINARY)
        msvcrt.setmode(out_fd, os.O_BINARY)
    devnull = os.open(os.devnull, os.O_RDONLY)
    os.dup2(devnull, 0)
    os.close(devnull)
    os.dup2(2, 1)
    sys.stdin = open(os.devnull, encoding="utf-8")
    sys.stdout = sys.stderr
    return os.fdopen(in_fd, "rb", buffering=0), os.fdopen(out_fd, "wb")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--family", required=True)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, stream=sys.stderr, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
    trace = os.environ.get("MELFIELD_WORKER_TRACE")
    if trace:
        # Diagnostics: dump every thread's stack to the log every N seconds.
        import faulthandler

        faulthandler.dump_traceback_later(float(trace), repeat=True, file=sys.stderr)
    channel = Channel(*_isolate_stdio())
    Worker(args.family, channel).run()


if __name__ == "__main__":
    main()
