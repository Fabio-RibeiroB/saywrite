from __future__ import annotations

import os
from pathlib import Path


def app_data_dir() -> Path:
    base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    path = base / "saywrite"
    path.mkdir(parents=True, exist_ok=True)
    return path


def local_models_dir() -> Path:
    path = app_data_dir() / "models"
    path.mkdir(parents=True, exist_ok=True)
    return path
