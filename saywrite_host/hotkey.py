from __future__ import annotations

from dataclasses import dataclass
import threading

import gi

gi.require_version("Atspi", "2.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Atspi, Gdk, GLib

from saywrite.cleanup import cleanup_transcript
from saywrite.config import AppSettings, load_settings
from saywrite.hardware import detect_local_runtime
from saywrite.transcription import transcribe_recorded_microphone


@dataclass(frozen=True)
class DictationShortcut:
    label: str
    keysym: int


DEFAULT_SHORTCUT = DictationShortcut(label="F8", keysym=Gdk.KEY_F8)


def shortcut_from_settings(settings: AppSettings) -> DictationShortcut:
    if settings.global_shortcut_label.upper() == "F8":
        return DEFAULT_SHORTCUT
    return DEFAULT_SHORTCUT


def extract_event_payload(args: tuple[object, ...]) -> object | None:
    for item in args:
        if hasattr(item, "type") and (hasattr(item, "id") or hasattr(item, "hw_code")):
            return item
    return None


def event_matches_shortcut(event: object | None, shortcut: DictationShortcut) -> bool:
    if event is None:
        return False
    event_type = getattr(event, "type", None)
    if event_type not in {Atspi.KeyEventType.PRESSED, int(Atspi.KeyEventType.PRESSED)}:
        return False
    event_id = getattr(event, "id", None)
    event_string = str(getattr(event, "event_string", "") or "")
    return event_id == shortcut.keysym or event_string.upper() == shortcut.label.upper()


class GlobalHotkeyDictation:
    def __init__(self, insertion_backend: object, duration_seconds: int = 5) -> None:
        self.insertion_backend = insertion_backend
        self.duration_seconds = duration_seconds
        self.listener: object | None = None
        self.loop: GLib.MainLoop | None = None
        self.shortcut = shortcut_from_settings(load_settings())
        self._lock = threading.Lock()
        self._busy = False

    def start(self) -> None:
        key = Atspi.KeyDefinition()
        key.keysym = self.shortcut.keysym
        key.keystring = self.shortcut.label
        key.modifiers = 0

        self.listener = Atspi.DeviceListener.new(self._on_device_event, None)
        Atspi.register_keystroke_listener(
            self.listener,
            [key],
            0,
            int(Atspi.KeyEventType.PRESSED),
            Atspi.KeyListenerSyncType.NOSYNC,
        )
        self.loop = GLib.MainLoop()
        threading.Thread(target=self.loop.run, daemon=True).start()
        print(f"SayWrite hotkey armed on {self.shortcut.label}. Keep focus in the target app and press it to dictate.")

    def stop(self) -> None:
        if self.listener is not None:
            key = Atspi.KeyDefinition()
            key.keysym = self.shortcut.keysym
            key.keystring = self.shortcut.label
            key.modifiers = 0
            Atspi.deregister_keystroke_listener(
                self.listener,
                [key],
                0,
                int(Atspi.KeyEventType.PRESSED),
            )
        if self.loop is not None:
            self.loop.quit()

    def _on_device_event(self, *args: object) -> bool:
        event = extract_event_payload(args)
        if not event_matches_shortcut(event, self.shortcut):
            return False
        with self._lock:
            if self._busy:
                return False
            self._busy = True
        threading.Thread(target=self._dictate_once, daemon=True).start()
        return False

    def _dictate_once(self) -> None:
        try:
            settings = load_settings()
            runtime = detect_local_runtime()
            raw_text = transcribe_recorded_microphone(
                settings.local_model_path,
                runtime.whisper_cli_path,
                duration_seconds=self.duration_seconds,
            )
            cleaned_text = cleanup_transcript(raw_text) if raw_text else ""
            if not cleaned_text:
                print("SayWrite hotkey dictation returned no transcript.")
                return
            status = self.insertion_backend.insert_text(cleaned_text)
            print(f"SayWrite hotkey dictation: {status}")
        except Exception as exc:
            print(f"SayWrite hotkey dictation failed: {type(exc).__name__}: {exc}")
        finally:
            with self._lock:
                self._busy = False
