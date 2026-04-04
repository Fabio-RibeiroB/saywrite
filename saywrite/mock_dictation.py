from __future__ import annotations


LOCAL_SEQUENCE = [
    "um can you",
    "um can you send me",
    "um can you send me the latest build notes",
    "um can you send me the latest build notes question mark",
]

CLOUD_SEQUENCE = [
    "please",
    "please send",
    "please send me the latest build notes",
    "please send me the latest build notes question mark",
]


class MockDictationController:
    def __init__(self, provider_mode: str = "local") -> None:
        self.provider_mode = provider_mode
        self._index = 0
        self._active = False

    def set_provider_mode(self, provider_mode: str) -> None:
        self.provider_mode = provider_mode
        self.reset()

    def start(self) -> str:
        self._active = True
        self._index = 0
        return self.current_chunk()

    def advance(self) -> str | None:
        if not self._active:
            return self.start()

        if self._index >= len(self._sequence()) - 1:
            self._active = False
            return None

        self._index += 1
        return self.current_chunk()

    def current_chunk(self) -> str:
        return self._sequence()[self._index]

    def is_active(self) -> bool:
        return self._active

    def reset(self) -> None:
        self._index = 0
        self._active = False

    def _sequence(self) -> list[str]:
        return LOCAL_SEQUENCE if self.provider_mode == "local" else CLOUD_SEQUENCE
