import unittest

from saywrite.cleanup import cleanup_transcript


class CleanupTranscriptTests(unittest.TestCase):
    def test_removes_fillers_and_adds_punctuation(self) -> None:
        raw = "uh can you send me the latest build notes question mark"
        self.assertEqual(cleanup_transcript(raw), "Can you send me the latest build notes?")

    def test_converts_spoken_symbols(self) -> None:
        raw = "open bracket super alt close bracket"
        self.assertEqual(cleanup_transcript(raw), "( super alt )")

    def test_supports_new_paragraph(self) -> None:
        raw = "first point new paragraph second point"
        self.assertEqual(cleanup_transcript(raw), "First point\n\nsecond point")


if __name__ == "__main__":
    unittest.main()
