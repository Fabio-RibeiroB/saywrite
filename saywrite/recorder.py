from __future__ import annotations

import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile


def record_microphone_clip(duration_seconds: int = 5, sample_rate: int = 16000) -> str:
    with tempfile.NamedTemporaryFile(prefix="saywrite-recording-", suffix=".wav", delete=False) as handle:
        output_path = Path(handle.name)

    result = subprocess.run(
        [
            "timeout",
            str(max(1, duration_seconds)),
            "gst-launch-1.0",
            "-q",
            "-e",
            "autoaudiosrc",
            "!",
            "audioconvert",
            "!",
            "audioresample",
            "!",
            f"audio/x-raw,rate={sample_rate},channels=1",
            "!",
            "wavenc",
            "!",
            "filesink",
            f"location={output_path}",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode not in {0, 124}:
        output_path.unlink(missing_ok=True)
        raise RuntimeError(result.stderr.strip() or "Microphone recording failed.")

    if not output_path.exists() or output_path.stat().st_size == 0:
        output_path.unlink(missing_ok=True)
        raise RuntimeError("Microphone recording produced no audio.")

    return str(output_path)


SESSION_FILE_NAME = "live-session.json"


def start_microphone_session(sample_rate: int = 16000) -> str:
    session_path = _session_path()
    if session_path.exists():
        raise RuntimeError("A dictation session is already running.")

    with tempfile.NamedTemporaryFile(prefix="saywrite-live-", suffix=".wav", delete=False) as handle:
        output_path = Path(handle.name)

    process = subprocess.Popen(
        [
            "gst-launch-1.0",
            "-q",
            "-e",
            "autoaudiosrc",
            "!",
            "audioconvert",
            "!",
            "audioresample",
            "!",
            f"audio/x-raw,rate={sample_rate},channels=1",
            "!",
            "wavenc",
            "!",
            "filesink",
            f"location={output_path}",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )

    session_path.write_text(
        json.dumps({"pid": process.pid, "audio_path": str(output_path)}),
        encoding="utf-8",
    )
    return str(output_path)


def stop_microphone_session() -> str:
    session_path = _session_path()
    if not session_path.exists():
        raise RuntimeError("No dictation session is running.")

    try:
        raw = json.loads(session_path.read_text(encoding="utf-8"))
        pid = int(raw["pid"])
        audio_path = Path(raw["audio_path"])
    except (OSError, KeyError, ValueError, json.JSONDecodeError) as exc:
        session_path.unlink(missing_ok=True)
        raise RuntimeError("Failed to read the active dictation session.") from exc

    try:
        os.kill(pid, signal.SIGINT)
    except ProcessLookupError:
        pass

    try:
        os.waitpid(pid, 0)
    except ChildProcessError:
        pass

    session_path.unlink(missing_ok=True)

    if not audio_path.exists() or audio_path.stat().st_size == 0:
        audio_path.unlink(missing_ok=True)
        raise RuntimeError("Microphone recording produced no audio.")

    return str(audio_path)


def session_active() -> bool:
    return _session_path().exists()


def _session_path() -> Path:
    runtime_dir = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
    runtime_dir.mkdir(parents=True, exist_ok=True)
    return runtime_dir / SESSION_FILE_NAME
