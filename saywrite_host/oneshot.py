from __future__ import annotations

import fcntl
import os
from pathlib import Path
import subprocess
from typing import TextIO

from saywrite.cleanup import cleanup_transcript
from saywrite.config import load_settings
from saywrite.hardware import detect_local_runtime
from saywrite.transcription import transcribe_recorded_microphone

from .backends import FallbackInsertionBackend


def lock_path() -> Path:
    runtime_dir = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
    runtime_dir.mkdir(parents=True, exist_ok=True)
    return runtime_dir / "saywrite-dictation.lock"


def acquire_lock() -> TextIO | None:
    handle = lock_path().open("w", encoding="utf-8")
    try:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        handle.close()
        return None
    return handle


def notify(summary: str, body: str) -> None:
    subprocess.run(
        ["notify-send", "-r", "31337", summary, body],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def run_dictation_once(duration_seconds: int = 5, insertion_backend: object | None = None) -> str:
    lock_handle = acquire_lock()
    if lock_handle is None:
        return "SayWrite dictation is already running."

    settings = load_settings()
    runtime = detect_local_runtime()

    try:
        if not settings.local_model_path.strip():
            raise RuntimeError("No local model configured in SayWrite.")
        if not runtime.whisper_cli_path:
            raise RuntimeError("whisper.cpp CLI is not configured.")

        backend = insertion_backend or FallbackInsertionBackend()
        notify("SayWrite", f"Listening for {duration_seconds} seconds...")
        raw_text = transcribe_recorded_microphone(
            settings.local_model_path,
            runtime.whisper_cli_path,
            duration_seconds=duration_seconds,
        )
        cleaned_text = cleanup_transcript(raw_text) if raw_text else ""
        if not cleaned_text:
            notify("SayWrite", "No transcript was produced.")
            return "No transcript was produced."

        status = backend.insert_text(cleaned_text)
        notify("SayWrite", status)
        return status
    finally:
        lock_handle.close()
