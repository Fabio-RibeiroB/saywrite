from __future__ import annotations

import json
import os
from pathlib import Path
import socket
import time

from .backends import FallbackInsertionBackend


SOCKET_NAME = "saywrite-host.sock"


def socket_path() -> Path:
    runtime_dir = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
    runtime_dir.mkdir(parents=True, exist_ok=True)
    return runtime_dir / SOCKET_NAME


class HostServer:
    def __init__(self) -> None:
        self.backend = FallbackInsertionBackend()
        self.path = socket_path()

    def serve_forever(self) -> None:
        if self.path.exists():
            self.path.unlink()

        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
            server.bind(str(self.path))
            server.listen()
            print(f"SayWrite host helper listening on {self.path}")
            while True:
                conn, _addr = server.accept()
                with conn:
                    raw = conn.recv(65536)
                    if not raw:
                        continue
                    response = self._handle(raw)
                    conn.sendall(response)

    def _handle(self, raw: bytes) -> bytes:
        try:
            message = json.loads(raw.decode("utf-8"))
            action = message.get("action")
            if action != "insert_text":
                raise RuntimeError(f"Unsupported action: {action}")
            text = str(message.get("text", ""))
            if not text.strip():
                raise RuntimeError("No text provided")
            delay_seconds = float(message.get("delay_seconds", 0))
            if delay_seconds < 0:
                raise RuntimeError("Delay must be non-negative")
            if delay_seconds > 0:
                time.sleep(delay_seconds)
            status = self.backend.insert_text(text)
            return json.dumps({"ok": True, "status": status}).encode("utf-8")
        except Exception as exc:
            return json.dumps({"ok": False, "error": str(exc)}).encode("utf-8")
