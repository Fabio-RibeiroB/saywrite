import unittest
from unittest.mock import MagicMock
from unittest.mock import patch

from saywrite.config import AppSettings
from saywrite.hardware import LocalRuntimeStatus
from saywrite_host.oneshot import run_dictation_once


class FakeInsertionBackend:
    def __init__(self) -> None:
        self.text = ""

    def insert_text(self, text: str) -> str:
        self.text = text
        return "typed"


class OneShotTests(unittest.TestCase):
    def test_run_dictation_once_transcribes_cleans_and_inserts(self) -> None:
        backend = FakeInsertionBackend()
        settings = AppSettings(local_model_path="/tmp/model.bin")
        runtime = LocalRuntimeStatus(
            gpu_vendor="amd",
            acceleration="vulkan",
            whisper_cli_path="/tmp/whisper-cli",
            cmake_available=True,
            vulkan_available=True,
            nvidia_smi_available=False,
        )

        fake_lock = MagicMock()
        with patch("saywrite_host.oneshot.acquire_lock", return_value=fake_lock), patch(
            "saywrite_host.oneshot.load_settings", return_value=settings
        ), patch(
            "saywrite_host.oneshot.detect_local_runtime", return_value=runtime
        ), patch(
            "saywrite_host.oneshot.transcribe_recorded_microphone", return_value="um hello there"
        ), patch("saywrite_host.oneshot.notify"):
            status = run_dictation_once(insertion_backend=backend)

        self.assertEqual(status, "typed")
        self.assertEqual(backend.text, "Hello there")
        fake_lock.close.assert_called_once()

    def test_run_dictation_once_refuses_reentry(self) -> None:
        with patch("saywrite_host.oneshot.acquire_lock", return_value=None), patch("saywrite_host.oneshot.notify") as notify:
            status = run_dictation_once()

        self.assertEqual(status, "SayWrite dictation is already running.")
        notify.assert_not_called()
