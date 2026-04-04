from __future__ import annotations

import argparse
import json
import sys

from .cleanup import cleanup_transcript
from .config import load_settings
from .hardware import detect_local_runtime
from .host_client import submit_text
from .recorder import session_active, start_microphone_session
from .transcription import transcribe_active_microphone_session, transcribe_recorded_microphone


def _transcribe_once(seconds: int) -> dict[str, object]:
    settings = load_settings()
    runtime = detect_local_runtime()

    if not settings.local_model_path.strip():
        raise RuntimeError("No local model configured in SayWrite.")
    if not runtime.whisper_cli_path:
        raise RuntimeError("whisper.cpp CLI is not configured.")

    raw_text = transcribe_recorded_microphone(
        settings.local_model_path,
        runtime.whisper_cli_path,
        duration_seconds=max(1, seconds),
    )
    cleaned_text = cleanup_transcript(raw_text) if raw_text else ""
    return {
        "ok": True,
        "raw_text": raw_text,
        "cleaned_text": cleaned_text,
        "model_path": settings.local_model_path,
        "whisper_cli_path": runtime.whisper_cli_path,
    }


def _send_text(text: str, delay_seconds: float) -> dict[str, object]:
    if not text.strip():
        raise RuntimeError("No text provided.")
    status = submit_text(text, delay_seconds=delay_seconds)
    return {"ok": True, "status": status}


def _start_live() -> dict[str, object]:
    if session_active():
        raise RuntimeError("A dictation session is already running.")
    start_microphone_session()
    return {"ok": True, "status": "Listening..."}


def _stop_live() -> dict[str, object]:
    settings = load_settings()
    runtime = detect_local_runtime()

    if not settings.local_model_path.strip():
        raise RuntimeError("No local model configured in SayWrite.")
    if not runtime.whisper_cli_path:
        raise RuntimeError("whisper.cpp CLI is not configured.")

    raw_text = transcribe_active_microphone_session(
        settings.local_model_path,
        runtime.whisper_cli_path,
    )
    cleaned_text = cleanup_transcript(raw_text) if raw_text else ""
    return {
        "ok": True,
        "raw_text": raw_text,
        "cleaned_text": cleaned_text,
        "model_path": settings.local_model_path,
        "whisper_cli_path": runtime.whisper_cli_path,
    }


def main() -> int:
    parser = argparse.ArgumentParser(prog="python3 -m saywrite.bridge_cli")
    subparsers = parser.add_subparsers(dest="command", required=True)

    transcribe = subparsers.add_parser("transcribe-once")
    transcribe.add_argument("--seconds", type=int, default=5)
    subparsers.add_parser("start-live")
    subparsers.add_parser("stop-live")

    send = subparsers.add_parser("send-text")
    send.add_argument("--text", required=True)
    send.add_argument("--delay-seconds", type=float, default=0.0)

    args = parser.parse_args()

    try:
        if args.command == "transcribe-once":
            payload = _transcribe_once(args.seconds)
        elif args.command == "start-live":
            payload = _start_live()
        elif args.command == "stop-live":
            payload = _stop_live()
        elif args.command == "send-text":
            payload = _send_text(args.text, args.delay_seconds)
        else:
            raise RuntimeError(f"Unsupported command: {args.command}")
    except Exception as exc:
        json.dump({"ok": False, "error": str(exc)}, sys.stdout)
        sys.stdout.write("\n")
        return 1

    json.dump(payload, sys.stdout)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
