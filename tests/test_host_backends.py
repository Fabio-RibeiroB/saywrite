import time
import unittest
from unittest.mock import patch

from saywrite_host.backends import (
    AccessibilityBackend,
    FallbackInsertionBackend,
    KeyboardEventBackend,
    find_focused_insertion_target,
)


class FakeStateSet:
    def __init__(self, focused: bool) -> None:
        self.focused = focused

    def contains(self, state: object) -> bool:
        return self.focused


class FakeEditableText:
    def __init__(self) -> None:
        self.calls: list[tuple[int, str, int]] = []

    def insert_text(self, position: int, text: str, length: int) -> bool:
        self.calls.append((position, text, length))
        return True


class FakeAccessible:
    def __init__(
        self,
        *,
        focused: bool = False,
        editable: FakeEditableText | None = None,
        caret_offset: int = 0,
        children: list["FakeAccessible"] | None = None,
    ) -> None:
        self._focused = focused
        self._editable = editable
        self._caret_offset = caret_offset
        self.children = children or []
        self.focus_grabbed = False

    def get_state_set(self) -> FakeStateSet:
        return FakeStateSet(self._focused)

    def get_editable_text_iface(self) -> FakeEditableText | None:
        return self._editable

    def get_caret_offset(self) -> int:
        return self._caret_offset

    def get_child_count(self) -> int:
        return len(self.children)

    def get_child_at_index(self, index: int) -> "FakeAccessible":
        return self.children[index]

    def grab_focus(self) -> bool:
        self.focus_grabbed = True
        return True


class FakeBackend:
    def __init__(self, *, name: str, error: str | None = None, status: str = "ok") -> None:
        self.name = name
        self.error = error
        self.status = status

    def insert_text(self, text: str) -> str:
        if self.error is not None:
            raise RuntimeError(self.error)
        return f"{self.status}: {text}"


class HostBackendTests(unittest.TestCase):
    def test_find_focused_insertion_target_returns_focused_editable(self) -> None:
        editable = FakeEditableText()
        root = FakeAccessible(
            children=[
                FakeAccessible(),
                FakeAccessible(focused=True, editable=editable, caret_offset=4),
            ]
        )

        target = find_focused_insertion_target([root])

        self.assertIsNotNone(target)
        assert target is not None
        self.assertEqual(target.caret_offset, 4)
        self.assertIs(target.editable, editable)

    def test_accessibility_backend_inserts_into_focused_field(self) -> None:
        editable = FakeEditableText()
        focused = FakeAccessible(focused=True, editable=editable, caret_offset=3)
        backend = AccessibilityBackend(desktops=[FakeAccessible(children=[focused])], poll_interval=0.01)

        try:
            status = backend.insert_text("hello")
        finally:
            backend.stop()

        self.assertEqual(status, "Text inserted into the last focused text field.")
        self.assertTrue(focused.focus_grabbed)
        self.assertEqual(editable.calls, [(3, "hello", 5)])

    def test_accessibility_backend_uses_remembered_target_after_focus_changes(self) -> None:
        editable = FakeEditableText()
        focused = FakeAccessible(focused=True, editable=editable, caret_offset=2)
        root = FakeAccessible(children=[focused])
        backend = AccessibilityBackend(desktops=[root], poll_interval=0.01)

        time_limit = 20
        while backend._remembered_target is None and time_limit > 0:
            time.sleep(0.01)
            time_limit -= 1

        focused._focused = False
        try:
            status = backend.insert_text("ok")
        finally:
            backend.stop()

        self.assertEqual(status, "Text inserted into the last focused text field.")
        self.assertEqual(editable.calls[-1], (2, "ok", 2))

    def test_fallback_backend_uses_next_backend(self) -> None:
        backend = FallbackInsertionBackend(
            backends=[
                FakeBackend(name="accessibility", error="no focused field"),
                FakeBackend(name="clipboard", status="clipboard"),
            ]
        )

        status = backend.insert_text("hello")

        self.assertEqual(status, "clipboard: hello")

    def test_keyboard_backend_sends_string_event(self) -> None:
        backend = KeyboardEventBackend()
        with patch("saywrite_host.backends.Atspi.generate_keyboard_event", return_value=True) as synth:
            status = backend.insert_text("hello")

        self.assertEqual(status, "Text typed into the currently focused app.")
        synth.assert_called_once()
