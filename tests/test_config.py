import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from saywrite.config import AppSettings, load_settings, save_settings


class ConfigTests(unittest.TestCase):
    def test_load_settings_returns_defaults_when_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with patch("saywrite.config.Path.home", return_value=Path(tmp)):
                settings = load_settings()
        self.assertEqual(settings, AppSettings())

    def test_save_and_load_roundtrip(self) -> None:
        expected = AppSettings(
            provider_mode="cloud",
            onboarding_complete=True,
            local_model_path="/tmp/model.bin",
            cloud_api_base="https://example.invalid/v1",
            cloud_api_key="secret",
            auto_copy_cleaned_text=False,
            auto_type_into_focused_app=True,
            global_shortcut_label="Super+Alt+D",
        )
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            with patch("saywrite.config.Path.home", return_value=home):
                save_settings(expected)
                settings = load_settings()
                stored = json.loads((home / ".config" / "saywrite" / "settings.json").read_text(encoding="utf-8"))

        self.assertEqual(settings, expected)
        self.assertEqual(stored["provider_mode"], "cloud")
        self.assertTrue(stored["onboarding_complete"])
        self.assertEqual(stored["local_model_path"], "/tmp/model.bin")
        self.assertEqual(stored["cloud_api_base"], "https://example.invalid/v1")
        self.assertEqual(stored["cloud_api_key"], "secret")
        self.assertFalse(stored["auto_copy_cleaned_text"])
        self.assertTrue(stored["auto_type_into_focused_app"])
        self.assertEqual(stored["global_shortcut_label"], "Super+Alt+D")
