use std::{
    env,
    path::{Path, PathBuf},
};

use crate::config::{default_model_path, AppSettings};

#[derive(Debug, Clone)]
pub struct RuntimeProbe {
    pub local_model_present: bool,
    pub local_model_display: String,
    pub whisper_cli_found: bool,
    pub whisper_cli_display: String,
    pub acceleration_label: String,
    pub insertion_label: String,
}

pub fn probe_runtime(settings: &AppSettings) -> RuntimeProbe {
    let model_path = preferred_model_path(settings);
    let whisper_cli = discover_whisper_cli();

    RuntimeProbe {
        local_model_present: model_path.exists(),
        local_model_display: shorten_path(&model_path),
        whisper_cli_found: whisper_cli.exists(),
        whisper_cli_display: shorten_path(&whisper_cli),
        acceleration_label: detect_acceleration(),
        insertion_label: "Host helper today, IBus next".into(),
    }
}

fn preferred_model_path(settings: &AppSettings) -> PathBuf {
    if settings.local_model_path.is_empty() {
        return default_model_path();
    }
    PathBuf::from(&settings.local_model_path)
}

fn discover_whisper_cli() -> PathBuf {
    if let Ok(path) = env::var("SAYWRITE_WHISPER_CLI") {
        return PathBuf::from(path);
    }

    let current = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let candidates = [
        current.join("vendor/whisper.cpp/build/bin/whisper-cli"),
        current.join("vendor/whisper.cpp/build/bin/Release/whisper-cli"),
        current.join("vendor/whisper.cpp/build/bin/main"),
    ];

    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| PathBuf::from("whisper-cli"))
}

fn shorten_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}

fn detect_acceleration() -> String {
    if let Ok(value) = env::var("SAYWRITE_ACCELERATION_HINT") {
        return value;
    }

    if let Ok(render) = env::var("DRI_PRIME") {
        if !render.is_empty() {
            return "GPU available".into();
        }
    }

    if env::var("NVIDIA_VISIBLE_DEVICES").is_ok() {
        return "CUDA candidate".into();
    }

    "Auto-detect at service start".into()
}
