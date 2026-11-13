"""Framed stdio protocol: 4-byte big-endian length + UTF-8 JSON; a header with
"bin": n is followed by one binary frame. Mirrors crates/melfield-core/src/engines/protocol.rs."""

from __future__ import annotations

import json
import struct
import threading
from typing import Any, BinaryIO

PROTOCOL_VERSION = 1
MAX_FRAME = 256 * 1024 * 1024


class Channel:
    """Thread-safe writer + blocking reader over binary stdio streams."""

    def __init__(self, reader: BinaryIO, writer: BinaryIO) -> None:
        self._reader = reader
        self._writer = writer
        self._lock = threading.Lock()

    def _write_frame(self, payload: bytes) -> None:
        self._writer.write(struct.pack(">I", len(payload)))
        self._writer.write(payload)

    def send(self, header: dict[str, Any], binary: bytes | None = None) -> None:
        if binary is not None:
            header = {**header, "bin": len(binary)}
        with self._lock:
            self._write_frame(json.dumps(header, ensure_ascii=False).encode("utf-8"))
            if binary is not None:
                self._write_frame(binary)
            self._writer.flush()

    def _read_exact(self, n: int) -> bytes:
        buf = bytearray()
        while len(buf) < n:
            chunk = self._reader.read(n - len(buf))
            if not chunk:
                raise EOFError("stdin closed")
            buf.extend(chunk)
        return bytes(buf)

    def recv(self) -> dict[str, Any]:
        (n,) = struct.unpack(">I", self._read_exact(4))
        if n > MAX_FRAME:
            raise ValueError(f"frame of {n} bytes exceeds limit")
        msg = json.loads(self._read_exact(n).decode("utf-8"))
        if not isinstance(msg, dict):
            raise ValueError("frame is not an object")
        return msg
