from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import subprocess

from .hardware import LocalRuntimeStatus, detect_local_runtime


@dataclass
class TranscriptionRequest:
    audio_path: str
    model_path: str
    language: str = "en"


@dataclass
class BackendProbe:
    provider_mode: str
    local_runtime: LocalRuntimeStatus
    local_model_configured: bool
    cloud_configured: bool


def probe_backends(local_model_path: str, cloud_api_key: str, provider_mode: str) -> BackendProbe:
    return BackendProbe(
        provider_mode=provider_mode,
        local_runtime=detect_local_runtime(),
        local_model_configured=bool(local_model_path.strip()),
        cloud_configured=bool(cloud_api_key.strip()),
    )


class WhisperCppBackend:
    def __init__(self, runtime: LocalRuntimeStatus | None = None) -> None:
        self.runtime = runtime or detect_local_runtime()

    def build_command(self, request: TranscriptionRequest) -> list[str]:
        if self.runtime.whisper_cli_path is None:
            raise RuntimeError("whisper.cpp runtime not found")
        cli_path = Path(self.runtime.whisper_cli_path)
        if not cli_path.exists():
            raise RuntimeError(f"whisper.cpp runtime path does not exist: {cli_path}")
        if cli_path.is_dir():
            raise RuntimeError(f"whisper.cpp runtime path is a directory, not an executable: {cli_path}")
        if not cli_path.is_file():
            raise RuntimeError(f"whisper.cpp runtime path is not a regular file: {cli_path}")

        return [
            str(cli_path),
            "--file",
            request.audio_path,
            "--model",
            request.model_path,
            "--language",
            request.language,
            "--no-timestamps",
        ]

    def transcribe(self, request: TranscriptionRequest) -> str:
        if not Path(request.audio_path).exists():
            raise FileNotFoundError(f"Audio file not found: {request.audio_path}")
        if not Path(request.model_path).exists():
            raise FileNotFoundError(f"Model file not found: {request.model_path}")

        result = subprocess.run(
            self.build_command(request),
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or "whisper.cpp transcription failed")

        return _extract_transcript(result.stdout)


def _extract_transcript(stdout: str) -> str:
    lines = []
    for raw_line in stdout.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        if line.startswith("whisper_") or line.startswith("system_info:") or line.startswith("main:"):
            continue
        if line.startswith("[") and "]" in line:
            _, content = line.split("]", 1)
            content = content.strip()
            if content:
                lines.append(content)
            continue
        lines.append(line)
    return " ".join(lines).strip()
