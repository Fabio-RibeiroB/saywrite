from __future__ import annotations

from pathlib import Path
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
