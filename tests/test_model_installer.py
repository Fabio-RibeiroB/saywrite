import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from saywrite.model_installer import default_model_path, install_default_model


class ModelInstallerTests(unittest.TestCase):
    def test_default_model_path_uses_local_models_dir(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            model_root = Path(tmp)
            with patch("saywrite.model_installer.local_models_dir", return_value=model_root):
                self.assertEqual(default_model_path(), model_root / "ggml-base.en.bin")

    def test_install_requires_vendor_script(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            model_root = Path(tmp) / "models"
            model_root.mkdir()
            with patch("saywrite.model_installer.local_models_dir", return_value=model_root):
                with self.assertRaisesRegex(RuntimeError, "model download script not found"):
                    install_default_model(tmp)
