import unittest
from unittest.mock import patch

from saywrite.backend import TranscriptionRequest, WhisperCppBackend, _extract_transcript, probe_backends
from saywrite.hardware import LocalRuntimeStatus


class BackendTests(unittest.TestCase):
    def test_build_command_uses_whisper_cli(self) -> None:
        runtime = LocalRuntimeStatus(
            gpu_vendor="amd",
            acceleration="vulkan",
            whisper_cli_path="/tmp/whisper-cli",
            cmake_available=True,
            vulkan_available=True,
            nvidia_smi_available=False,
        )
        backend = WhisperCppBackend(runtime)
        with patch("saywrite.backend.Path.exists", return_value=True), patch(
            "saywrite.backend.Path.is_dir", return_value=False
        ), patch("saywrite.backend.Path.is_file", return_value=True):
            command = backend.build_command(TranscriptionRequest("/tmp/in.wav", "/tmp/model.bin"))
        self.assertEqual(
            command,
            [
                "/tmp/whisper-cli",
                "--file",
                "/tmp/in.wav",
                "--model",
                "/tmp/model.bin",
                "--language",
                "en",
                "--no-timestamps",
            ],
        )

    def test_extract_transcript_ignores_logs(self) -> None:
        stdout = "\n".join(
            [
                "system_info: mock",
                "main: processing",
                "[00:00:00.000 --> 00:00:01.000] hello there",
                "[00:00:01.000 --> 00:00:02.000] general kenobi",
            ]
        )
        self.assertEqual(_extract_transcript(stdout), "hello there general kenobi")

    def test_probe_backends_tracks_configuration(self) -> None:
        probe = probe_backends("/tmp/model.bin", "secret", "local")
        self.assertEqual(probe.provider_mode, "local")
        self.assertTrue(probe.local_model_configured)
        self.assertTrue(probe.cloud_configured)
