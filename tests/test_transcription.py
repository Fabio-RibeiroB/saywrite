import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from saywrite.transcription import run_local_transcription, transcribe_recorded_microphone, write_sine_wave


class TranscriptionTests(unittest.TestCase):
    def test_write_sine_wave_creates_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "tone.wav"
            write_sine_wave(str(path))
            self.assertTrue(path.exists())
            self.assertGreater(path.stat().st_size, 0)

    def test_run_local_transcription_requires_model_path(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "No local model configured"):
            run_local_transcription("", "/tmp/whisper-cli")

    def test_run_local_transcription_requires_cli(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "whisper.cpp CLI is not configured"):
            run_local_transcription("/tmp/model.bin", None)

    def test_run_local_transcription_uses_backend(self) -> None:
        with patch("saywrite.transcription.WhisperCppBackend.transcribe", return_value="hello world"):
            self.assertEqual(run_local_transcription("/tmp/model.bin", "/tmp/whisper-cli"), "hello world")

    def test_transcribe_recorded_microphone_requires_model_path(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "No local model configured"):
            transcribe_recorded_microphone("", "/tmp/whisper-cli")

    def test_transcribe_recorded_microphone_uses_backend(self) -> None:
        with patch("saywrite.transcription.record_microphone_clip", return_value="/tmp/in.wav"), patch(
            "saywrite.transcription.Path.unlink", return_value=None
        ), patch("saywrite.transcription.WhisperCppBackend.transcribe", return_value="spoken words"):
            self.assertEqual(
                transcribe_recorded_microphone("/tmp/model.bin", "/tmp/whisper-cli"),
                "spoken words",
            )
