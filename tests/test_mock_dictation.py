import unittest

from saywrite.mock_dictation import MockDictationController


class MockDictationControllerTests(unittest.TestCase):
    def test_local_sequence_starts_with_local_copy(self) -> None:
        controller = MockDictationController("local")
        self.assertEqual(controller.start(), "um can you")

    def test_cloud_sequence_changes_when_provider_changes(self) -> None:
        controller = MockDictationController("local")
        controller.set_provider_mode("cloud")
        self.assertEqual(controller.start(), "please")

    def test_advance_finishes_session(self) -> None:
        controller = MockDictationController("local")
        controller.start()
        controller.advance()
        controller.advance()
        controller.advance()
        self.assertIsNone(controller.advance())
        self.assertFalse(controller.is_active())
