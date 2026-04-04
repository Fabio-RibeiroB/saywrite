from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from pathlib import Path

from .paths import local_models_dir


APP_DIR_NAME = "saywrite"
SETTINGS_FILE_NAME = "settings.json"


@dataclass
class AppSettings:
    provider_mode: str = "local"
    onboarding_complete: bool = False
    local_model_path: str = ""
    cloud_api_base: str = "https://api.openai.com/v1"
    cloud_api_key: str = ""
    auto_copy_cleaned_text: bool = True
    auto_type_into_focused_app: bool = False
    global_shortcut_label: str = "Super+Alt+D"


def _settings_path() -> Path:
    config_home = Path.home() / ".config"
    app_dir = config_home / APP_DIR_NAME
    app_dir.mkdir(parents=True, exist_ok=True)
    return app_dir / SETTINGS_FILE_NAME


def load_settings() -> AppSettings:
    default_model = local_models_dir() / "ggml-base.en.bin"

    path = _settings_path()
    if not path.exists():
        return AppSettings(local_model_path=str(default_model) if default_model.exists() else "")

    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return AppSettings()

    local_model_path = raw.get("local_model_path", "")
    if not local_model_path and default_model.exists():
        local_model_path = str(default_model)

    return AppSettings(
        provider_mode=raw.get("provider_mode", "local"),
        onboarding_complete=bool(raw.get("onboarding_complete", False)),
        local_model_path=local_model_path,
        cloud_api_base=raw.get("cloud_api_base", "https://api.openai.com/v1"),
        cloud_api_key=raw.get("cloud_api_key", ""),
        auto_copy_cleaned_text=bool(raw.get("auto_copy_cleaned_text", True)),
        auto_type_into_focused_app=bool(raw.get("auto_type_into_focused_app", False)),
        global_shortcut_label=raw.get("global_shortcut_label", "Super+Alt+D"),
    )


def save_settings(settings: AppSettings) -> None:
    path = _settings_path()
    path.write_text(json.dumps(asdict(settings), indent=2), encoding="utf-8")
