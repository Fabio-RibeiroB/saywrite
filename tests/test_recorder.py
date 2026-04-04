import unittest

from saywrite.recorder import record_microphone_clip


class RecorderTests(unittest.TestCase):
    def test_function_exists(self) -> None:
        self.assertTrue(callable(record_microphone_clip))
