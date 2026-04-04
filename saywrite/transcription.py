from __future__ import annotations

from pathlib import Path
import tempfile
import wave

from .backend import TranscriptionRequest, WhisperCppBackend
from .recorder import record_microphone_clip, stop_microphone_session


def write_sine_wave(path: str, duration_seconds: float = 1.0, sample_rate: int = 16000) -> None:
    import math
    import struct

    total_frames = int(duration_seconds * sample_rate)
    with wave.open(path, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        for frame in range(total_frames):
            value = int(32767 * 0.15 * math.sin(2 * math.pi * 440 * (frame / sample_rate)))
            wav.writeframes(struct.pack("<h", value))


def run_local_transcription(model_path: str, whisper_cli_path: str | None) -> str:
    if not model_path.strip():
        raise RuntimeError("No local model configured.")
    if not whisper_cli_path:
        raise RuntimeError("whisper.cpp CLI is not configured.")

    backend = WhisperCppBackend()
    backend.runtime.whisper_cli_path = whisper_cli_path

    with tempfile.TemporaryDirectory() as tmp:
        audio_path = Path(tmp) / "sample.wav"
        write_sine_wave(str(audio_path))
        return backend.transcribe(TranscriptionRequest(str(audio_path), model_path))


def transcribe_recorded_microphone(model_path: str, whisper_cli_path: str | None, duration_seconds: int = 5) -> str:
    if not model_path.strip():
        raise RuntimeError("No local model configured.")
    if not whisper_cli_path:
        raise RuntimeError("whisper.cpp CLI is not configured.")

    backend = WhisperCppBackend()
    backend.runtime.whisper_cli_path = whisper_cli_path

    audio_path = Path(record_microphone_clip(duration_seconds=duration_seconds))
    try:
        return backend.transcribe(TranscriptionRequest(str(audio_path), model_path))
    finally:
        audio_path.unlink(missing_ok=True)


def transcribe_active_microphone_session(model_path: str, whisper_cli_path: str | None) -> str:
    if not model_path.strip():
        raise RuntimeError("No local model configured.")
    if not whisper_cli_path:
        raise RuntimeError("whisper.cpp CLI is not configured.")

    backend = WhisperCppBackend()
    backend.runtime.whisper_cli_path = whisper_cli_path

    audio_path = Path(stop_microphone_session())
    try:
        return backend.transcribe(TranscriptionRequest(str(audio_path), model_path))
    finally:
        audio_path.unlink(missing_ok=True)
