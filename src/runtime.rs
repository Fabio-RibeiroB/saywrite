use std::{env, path::Path, process::Command};

use crate::{
    config::{preferred_model_path, AppSettings, ProviderMode},
    dictation, host_integration,
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
        insertion_label: match host_integration::host_status() {
            Some(status) => format!(
                "{} via {}",
                crate::host_api::insertion_capability_label(&status.insertion_capability),
                status.insertion_backend
            ),
            None => "Clipboard fallback until host integration is running".into(),
        },
        dictation_label: match host_integration::host_status() {
            Some(status) if status.hotkey_active => "Host daemon ready for hotkey dictation".into(),
            Some(_) => "Host daemon running, but the global shortcut is not active yet".into(),
            None => "Install host companion for global hotkey dictation".into(),
        },
        provider_label,
    }
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

    if let Some(gpu_name) = detect_gpu_name() {
        return format!(
            "GPU detected ({gpu_name}). whisper.cpp decides CPU vs GPU at transcription time."
        );
    }

    "No GPU details detected. whisper.cpp will decide CPU or GPU at transcription time.".into()
}

fn detect_gpu_name() -> Option<String> {
    let output = Command::new("lspci").output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            if !lower.contains("vga compatible controller")
                && !lower.contains("3d controller")
                && !lower.contains("display controller")
            {
                return None;
            }
            let (_, value) = line.split_once(':')?;
            let name = value.trim();
            (!name.is_empty()).then(|| name.to_string())
        })
}
