from __future__ import annotations

from typing import Callable

import gi

gi.require_version("Gst", "1.0")
from gi.repository import Gst


def rms_to_percent(rms_db: float) -> int:
    if rms_db <= -60:
        return 0
    if rms_db >= 0:
        return 100
    return max(0, min(100, int(((rms_db + 60) / 60) * 100)))


class MicrophoneMonitor:
    def __init__(
        self,
        on_level: Callable[[int], None],
        on_state: Callable[[str], None],
    ) -> None:
        Gst.init(None)
        self._on_level = on_level
        self._on_state = on_state
        self._pipeline: Gst.Pipeline | None = None
        self._bus: Gst.Bus | None = None
        self._bus_handler_id: int | None = None

    def start(self) -> None:
        if self._pipeline is not None:
            return

        self._pipeline = Gst.parse_launch(
            "autoaudiosrc ! audioconvert ! audioresample ! "
            "level interval=100000000 post-messages=true ! fakesink"
        )
        self._bus = self._pipeline.get_bus()
        if self._bus is not None:
            self._bus.add_signal_watch()
            self._bus_handler_id = self._bus.connect("message", self._on_message)

        result = self._pipeline.set_state(Gst.State.PLAYING)
        if result == Gst.StateChangeReturn.FAILURE:
            self.stop()
            self._on_state("Could not start microphone capture.")
            return

        self._on_state("Listening for microphone input. The Flatpak prompt should appear the first time capture starts.")

    def stop(self) -> None:
        if self._pipeline is not None:
            self._pipeline.set_state(Gst.State.NULL)

        if self._bus is not None:
            if self._bus_handler_id is not None:
                self._bus.disconnect(self._bus_handler_id)
            self._bus.remove_signal_watch()

        self._pipeline = None
        self._bus = None
        self._bus_handler_id = None
        self._on_level(0)
        self._on_state("Microphone monitor is stopped.")

    def is_running(self) -> bool:
        return self._pipeline is not None

    def _on_message(self, _bus: Gst.Bus, message: Gst.Message) -> None:
        if message.type == Gst.MessageType.ERROR:
            error, _debug = message.parse_error()
            self.stop()
            self._on_state(f"Microphone error: {error.message}")
            return

        if message.type != Gst.MessageType.ELEMENT:
            return

        structure = message.get_structure()
        if structure is None or structure.get_name() != "level":
            return

        rms_values = structure.get_value("rms")
        if not rms_values:
            return

        rms_value = float(rms_values[0])
        self._on_level(rms_to_percent(rms_value))
