import unittest

from saywrite.audio import rms_to_percent


class AudioTests(unittest.TestCase):
    def test_rms_to_percent_handles_floor(self) -> None:
        self.assertEqual(rms_to_percent(-80), 0)

    def test_rms_to_percent_handles_ceiling(self) -> None:
        self.assertEqual(rms_to_percent(3), 100)

    def test_rms_to_percent_scales_midpoint(self) -> None:
        self.assertEqual(rms_to_percent(-30), 50)
