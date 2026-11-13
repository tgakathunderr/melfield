"""Drive a real worker process over stdio with the fake family.

Run: uv run --with numpy --with pytest pytest workers/tests -q
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import sys

import numpy as np
import pytest

WORKERS = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(WORKERS))

from melfield_worker.protocol import Channel  # noqa: E402


class Proc:
    def __init__(self, family: str = "fake") -> None:
        env = {**os.environ, "PYTHONPATH": str(WORKERS), "PYTHONIOENCODING": "utf-8"}
        self.p = subprocess.Popen(
            [sys.executable, "-u", "-m", "melfield_worker", "--family", family],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env,
        )
        assert self.p.stdin and self.p.stdout
        self.ch = Channel(self.p.stdout, self.p.stdin)
        self.next_id = 1

    def call(self, op: dict, cancel_after: int | None = None) -> tuple[dict, list[tuple[dict, bytes | None]]]:
        rid = self.next_id
        self.next_id += 1
        self.ch.send({**op, "id": rid})
        events = []
        while True:
            header = self.ch.recv()
            binary = None
            if "bin" in header:
                import struct

                (n,) = struct.unpack(">I", self.ch._read_exact(4))
                binary = self.ch._read_exact(n)
            if header.get("id") != rid:
                continue
            if "event" in header:
                events.append((header, binary))
                if cancel_after is not None and len(events) == cancel_after:
                    self.ch.send({"id": 999, "op": "cancel", "target": rid})
                continue
            return header, events

    def close(self) -> None:
        try:
            self.ch.send({"id": 0, "op": "shutdown"})
            self.p.wait(timeout=5)
        except Exception:
            self.p.kill()


@pytest.fixture()
def worker():
    w = Proc()
    yield w
    w.close()


def load(w: Proc, entry: str = "default") -> dict:
    reply, _ = w.call({"op": "load", "recipe": "fake", "entry": entry, "device": "cpu", "sample_rate": 24000, "artifacts": []})
    return reply


def synth(text: str, lang: str = "en", watermark: bool = False) -> dict:
    return {"op": "synthesize", "text": text, "lang": lang, "voice": {"preset": "x"}, "speed": 1.0, "delivery": "natural", "watermark": watermark}


def test_hello_reports_family_and_protocol(worker: Proc) -> None:
    reply, _ = worker.call({"op": "hello", "protocol": 1})
    assert reply["ok"] and reply["family"] == "fake" and reply["protocol"] == 1


def test_streams_float_chunks(worker: Proc) -> None:
    assert load(worker)["ok"]
    reply, events = worker.call(synth("hello there, this is a test"))
    assert reply["ok"] and reply["watermarked"] is False
    audio = np.concatenate([np.frombuffer(b, dtype="<f4") for _, b in events if b])
    assert len(events) > 1
    assert abs(len(audio) / 24000 - len("hello there, this is a test") / 15) < 0.3
    assert 0.1 < float(np.sqrt(np.mean(audio**2))) < 0.4


def test_cancel_mid_stream(worker: Proc) -> None:
    load(worker)
    reply, events = worker.call(synth("x" * 300), cancel_after=1)
    assert reply["ok"] is False and reply["error"]["code"] == "CANCELLED"
    assert len(events) < 10


def test_error_codes_are_stable(worker: Proc) -> None:
    reply, _ = worker.call(synth("hi"))
    assert reply["error"]["code"] == "MODEL_FILES_MISSING"
    assert load(worker, entry="missing")["error"]["code"] == "MODEL_FILES_MISSING"
    load(worker)
    assert worker.call(synth("hi", lang="xx"))[0]["error"]["code"] == "UNSUPPORTED_LANGUAGE"
    assert worker.call(synth("__oom__"))[0]["error"]["code"] == "OUT_OF_MEMORY"
    assert worker.call({"op": "bogus"})[0]["error"]["code"] == "INTERNAL"
    # Still alive after errors.
    assert worker.call(synth("ok"))[0]["ok"]


def test_clone_requests_refuse_without_watermarker(worker: Proc) -> None:
    pytest.importorskip("numpy")
    try:
        import perth  # noqa: F401

        pytest.skip("perth installed; refusal path not reachable here")
    except ImportError:
        pass
    load(worker)
    reply, events = worker.call(synth("clone me", watermark=True))
    assert reply["ok"] is False and reply["error"]["code"] == "WATERMARK_UNAVAILABLE"
    assert events == []


def test_stray_prints_do_not_corrupt_channel(worker: Proc) -> None:
    # The worker redirects stdout to stderr; a library print must not break framing.
    load(worker)
    for _ in range(3):
        assert worker.call(synth("again"))[0]["ok"]
