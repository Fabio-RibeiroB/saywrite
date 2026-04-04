from __future__ import annotations

import json
import os
from pathlib import Path
import socket


SOCKET_NAME = "saywrite-host.sock"


def socket_path() -> Path:
    runtime_dir = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
    return runtime_dir / SOCKET_NAME


def submit_text(text: str, delay_seconds: float = 0.0) -> str:
    path = socket_path()
    if not path.exists():
        raise RuntimeError(f"Host helper not running: {path} not found")

    payload = json.dumps(
        {
            "action": "insert_text",
            "text": text,
            "delay_seconds": delay_seconds,
        }
    ).encode("utf-8")
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(str(path))
        client.sendall(payload)
        client.shutdown(socket.SHUT_WR)
        response = client.recv(65536)

    message = json.loads(response.decode("utf-8"))
    if message.get("ok"):
        return str(message.get("status", "ok"))
    raise RuntimeError(str(message.get("error", "unknown host helper error")))
