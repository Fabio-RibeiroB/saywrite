import gi
import unittest

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi

from saywrite.config import AppSettings
from saywrite_host.hotkey import DEFAULT_SHORTCUT, event_matches_shortcut, extract_event_payload, shortcut_from_settings


class FakeEvent:
    def __init__(self, *, type: int, id: int, event_string: str) -> None:
        self.type = type
        self.id = id
        self.event_string = event_string


class HotkeyTests(unittest.TestCase):
    def test_extract_event_payload_finds_event_like_object(self) -> None:
        event = FakeEvent(type=1, id=2, event_string="F8")
        self.assertIs(extract_event_payload(("x", event, None)), event)

    def test_event_matches_shortcut_accepts_matching_f8_press(self) -> None:
        event = FakeEvent(type=int(Atspi.KeyEventType.PRESSED), id=DEFAULT_SHORTCUT.keysym, event_string="F8")
        self.assertTrue(event_matches_shortcut(event, DEFAULT_SHORTCUT))

    def test_shortcut_from_settings_defaults_to_f8(self) -> None:
        self.assertEqual(shortcut_from_settings(AppSettings()), DEFAULT_SHORTCUT)
