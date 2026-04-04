import tempfile
import unittest
from pathlib import Path
import json
from unittest.mock import patch

from saywrite.host_client import socket_path, submit_text


class FakeSocket:
    def __init__(self) -> None:
        self.sent = b""

    def connect(self, _path: str) -> None:
        return None

    def sendall(self, payload: bytes) -> None:
        self.sent = payload

    def shutdown(self, _how: int) -> None:
        return None

    def recv(self, _size: int) -> bytes:
        return json.dumps({"ok": True, "status": "typed"}).encode("utf-8")

    def __enter__(self) -> "FakeSocket":
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> bool:
        return False


class HostClientTests(unittest.TestCase):
    def test_socket_path_uses_runtime_dir(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict("os.environ", {"XDG_RUNTIME_DIR": tmp}, clear=True):
                self.assertEqual(socket_path(), Path(tmp) / "saywrite-host.sock")

    def test_submit_text_includes_delay_seconds(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            fake_socket = FakeSocket()
            with patch.dict("os.environ", {"XDG_RUNTIME_DIR": tmp}, clear=True):
                socket_file = Path(tmp) / "saywrite-host.sock"
                socket_file.write_text("", encoding="utf-8")
                with patch("saywrite.host_client.socket.socket", return_value=fake_socket):
                    status = submit_text("hello", delay_seconds=3.0)

        self.assertEqual(status, "typed")
        payload = json.loads(fake_socket.sent.decode("utf-8"))
        self.assertEqual(payload["text"], "hello")
        self.assertEqual(payload["delay_seconds"], 3.0)
