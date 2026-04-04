import os
import stat
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from saywrite.hardware import detect_gpu_vendor, detect_local_runtime, detect_whisper_cli


class HardwareTests(unittest.TestCase):
    def test_detect_whisper_cli_uses_environment_override(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp) / "whisper-cli"
            binary.write_text("", encoding="utf-8")
            binary.chmod(binary.stat().st_mode | stat.S_IXUSR)
            with patch.dict(os.environ, {"SAYWRITE_WHISPER_CLI": str(binary)}, clear=True):
                self.assertEqual(detect_whisper_cli(), str(binary))

    def test_detect_whisper_cli_finds_repo_build(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo_root = Path(tmp)
            candidate = repo_root / "vendor" / "whisper.cpp" / "build" / "bin" / "whisper-cli"
            candidate.parent.mkdir(parents=True)
            candidate.write_text("", encoding="utf-8")
            candidate.chmod(candidate.stat().st_mode | stat.S_IXUSR)
            fake_file = repo_root / "saywrite" / "hardware.py"
            fake_file.parent.mkdir(parents=True)
            fake_file.write_text("", encoding="utf-8")
            with patch("saywrite.hardware.__file__", str(fake_file)), patch.dict(os.environ, {}, clear=True):
                self.assertEqual(detect_whisper_cli(), str(candidate))

    def test_detect_gpu_vendor_prefers_nvidia_smi(self) -> None:
        with patch("saywrite.hardware.shutil.which", side_effect=lambda cmd: "/usr/bin/nvidia-smi" if cmd == "nvidia-smi" else None):
            self.assertEqual(detect_gpu_vendor(), "nvidia")

    def test_detect_local_runtime_picks_vulkan_for_amd(self) -> None:
        with patch("saywrite.hardware.detect_gpu_vendor", return_value="amd"), patch(
            "saywrite.hardware.detect_whisper_cli", return_value="/tmp/whisper-cli"
        ), patch("saywrite.hardware.shutil.which", side_effect=lambda cmd: "/usr/bin/cmake" if cmd == "cmake" else None), patch(
            "saywrite.hardware._command_ok", return_value=True
        ):
            runtime = detect_local_runtime()
        self.assertEqual(runtime.acceleration, "vulkan")
