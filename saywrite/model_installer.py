from __future__ import annotations

from pathlib import Path
import shutil
import subprocess

from .paths import local_models_dir


DEFAULT_MODEL_NAME = "base.en"
DEFAULT_MODEL_FILENAME = "ggml-base.en.bin"


def default_model_path() -> Path:
    return local_models_dir() / DEFAULT_MODEL_FILENAME


def install_default_model(project_root: str) -> Path:
    target_dir = local_models_dir()
    vendor_script = Path(project_root) / "vendor" / "whisper.cpp" / "models" / "download-ggml-model.sh"
    if not vendor_script.exists():
        raise RuntimeError("whisper.cpp model download script not found. Build whisper.cpp first.")

    if shutil.which("bash") is None:
        raise RuntimeError("bash is required to run the whisper.cpp model downloader.")

    result = subprocess.run(
        ["bash", str(vendor_script), DEFAULT_MODEL_NAME, str(target_dir)],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip() or "Model download failed.")

    model_path = default_model_path()
    if not model_path.exists():
        raise RuntimeError(f"Model download completed but {model_path} was not found.")
    return model_path
