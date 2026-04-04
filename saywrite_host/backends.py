from __future__ import annotations

from dataclasses import dataclass
import threading
import time

import gi

gi.require_version("Atspi", "2.0")
gi.require_version("Gdk", "4.0")
gi.require_version("Gtk", "4.0")
from gi.repository import Atspi, Gdk, Gtk


@dataclass
class InsertionTarget:
    accessible: object
    editable: object
    caret_offset: int


def _child_count(accessible: object) -> int:
    getter = getattr(accessible, "get_child_count", None)
    if callable(getter):
        return int(getter())
    children = getattr(accessible, "children", None)
    if children is None:
        return 0
    return len(children)


def _child_at(accessible: object, index: int) -> object | None:
    getter = getattr(accessible, "get_child_at_index", None)
    if callable(getter):
        return getter(index)
    children = getattr(accessible, "children", None)
    if children is None:
        return None
    return children[index]


def _state_contains(accessible: object, state: object) -> bool:
    state_set = accessible.get_state_set()
    return bool(state_set and state_set.contains(state))


def _editable_iface(accessible: object) -> object | None:
    getter = getattr(accessible, "get_editable_text_iface", None)
    if not callable(getter):
        return None
    return getter()


def _caret_offset(accessible: object) -> int:
    getter = getattr(accessible, "get_caret_offset", None)
    if callable(getter):
        try:
            offset = int(getter())
        except Exception:
            offset = -1
        if offset >= 0:
            return offset

    text_getter = getattr(accessible, "get_text_iface", None)
    if callable(text_getter):
        text_iface = text_getter()
        if text_iface is not None:
            try:
                offset = int(text_iface.get_caret_offset())
            except Exception:
                offset = -1
            if offset >= 0:
                return offset
    return 0


def _walk_accessibles(root: object) -> list[object]:
    nodes = [root]
    for index in range(_child_count(root)):
        child = _child_at(root, index)
        if child is None:
            continue
        nodes.extend(_walk_accessibles(child))
    return nodes


def find_focused_insertion_target(desktops: list[object]) -> InsertionTarget | None:
    for desktop in desktops:
        for accessible in _walk_accessibles(desktop):
            if not _state_contains(accessible, Atspi.StateType.FOCUSED):
                continue
            editable = _editable_iface(accessible)
            if editable is None:
                continue
            return InsertionTarget(
                accessible=accessible,
                editable=editable,
                caret_offset=_caret_offset(accessible),
            )
    return None


class AccessibilityBackend:
    name = "accessibility"

    def __init__(self, desktops: list[object] | None = None, poll_interval: float = 0.25) -> None:
        self.desktops = desktops
        self.poll_interval = poll_interval
        self._remembered_target: InsertionTarget | None = None
        self._stop_event = threading.Event()
        self._watcher = threading.Thread(target=self._watch_focus_loop, daemon=True)
        self._watcher.start()

    def _desktops(self) -> list[object]:
        return self.desktops if self.desktops is not None else list(Atspi.get_desktop_list())

    def _watch_focus_loop(self) -> None:
        while not self._stop_event.is_set():
            try:
                target = find_focused_insertion_target(self._desktops())
            except Exception:
                target = None
            if target is not None:
                self._remembered_target = target
            self._stop_event.wait(self.poll_interval)

    def stop(self) -> None:
        self._stop_event.set()
        if self._watcher.is_alive():
            self._watcher.join(timeout=0.5)

    def _resolve_target(self) -> InsertionTarget | None:
        current = find_focused_insertion_target(self._desktops())
        if current is not None:
            self._remembered_target = current
            return current
        return self._remembered_target

    def insert_text(self, text: str) -> str:
        target = self._resolve_target()
        if target is None:
            raise RuntimeError("No editable text field has been focused yet")

        grab_focus = getattr(target.accessible, "grab_focus", None)
        if callable(grab_focus):
            try:
                grab_focus()
            except Exception:
                pass

        inserted = bool(target.editable.insert_text(target.caret_offset, text, len(text)))
        if not inserted:
            raise RuntimeError("Focused text field rejected accessibility text insertion")
        return "Text inserted into the last focused text field."


class ClipboardBackend:
    name = "clipboard"

    def insert_text(self, text: str) -> str:
        Gtk.init()
        display = Gdk.Display.get_default()
        if display is None:
            raise RuntimeError("No display available for clipboard backend")
        clipboard = display.get_clipboard()
        clipboard.set(text)
        return "Focused-field typing unavailable. Text copied to clipboard instead."


class KeyboardEventBackend:
    name = "keyboard"

    def insert_text(self, text: str) -> str:
        sent = bool(Atspi.generate_keyboard_event(0, text, Atspi.KeySynthType.STRING))
        if not sent:
            raise RuntimeError("Keyboard event synthesis failed")
        return "Text typed into the currently focused app."


class FallbackInsertionBackend:
    def __init__(self, backends: list[object] | None = None) -> None:
        self.backends = (
            backends
            if backends is not None
            else [AccessibilityBackend(), KeyboardEventBackend(), ClipboardBackend()]
        )

    def insert_text(self, text: str) -> str:
        errors: list[str] = []
        for backend in self.backends:
            try:
                return backend.insert_text(text)
            except Exception as exc:
                errors.append(f"{backend.name}: {exc}")
        raise RuntimeError("All insertion backends failed: " + "; ".join(errors))
