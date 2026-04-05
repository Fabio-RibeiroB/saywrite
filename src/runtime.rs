use std::{
    env,
    path::{Path, PathBuf},
};

use crate::{
    config::{default_model_path, AppSettings, ProviderMode},
    dictation,
    host_integration,
};

#[derive(Debug, Clone)]
pub struct RuntimeProbe {
    pub local_model_present: bool,
    pub local_model_display: String,
    pub whisper_cli_found: bool,
    pub whisper_cli_display: String,
    pub acceleration_label: String,
    pub insertion_label: String,
    pub dictation_label: String,
    pub provider_label: String,
}

pub fn probe_runtime(settings: &AppSettings) -> RuntimeProbe {
    let model_path = preferred_model_path(settings);
    let whisper_cli = dictation::discover_whisper_cli();
    let provider_label = match settings.provider_mode {
        ProviderMode::Cloud => "Cloud".into(),
        ProviderMode::Local => "Local".into(),
    };

    RuntimeProbe {
        local_model_present: model_path.exists(),
        local_model_display: shorten_path(&model_path),
        whisper_cli_found: whisper_cli.exists(),
        whisper_cli_display: shorten_path(&whisper_cli),
        acceleration_label: detect_acceleration(),
        insertion_label: if host_integration::host_available() {
            "Host integration online".into()
        } else {
            "Clipboard fallback until host integration is running".into()
        },
        dictation_label: if dictation::active_session() {
            "Live dictation session active".into()
        } else {
            "Ready for local dictation".into()
        },
        provider_label,
    }
}

fn preferred_model_path(settings: &AppSettings) -> PathBuf {
    if let Some(path) = &settings.local_model_path {
        return path.clone();
    }
    default_model_path()
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
