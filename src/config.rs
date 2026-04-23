use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub const APP_DIR_NAME: &str = "saywrite";
const SETTINGS_FILE_NAME: &str = "settings.json";
const DEFAULT_CLOUD_API_BASE: &str = "https://api.openai.com/v1";
const LEGACY_SHORTCUT: &str = "F8";
const DEFAULT_SHORTCUT: &str = "Super+Alt+D";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderMode {
    Local,
    Cloud,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelSize {
    Tiny,
    #[default]
    Base,
    Small,
}

impl ModelSize {
    pub fn filename(self) -> &'static str {
        match self {
            ModelSize::Tiny => "ggml-tiny.en.bin",
            ModelSize::Base => "ggml-base.en.bin",
            ModelSize::Small => "ggml-small.en.bin",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ModelSize::Tiny => "tiny.en (~75 MB)",
            ModelSize::Base => "base.en (~142 MB)",
            ModelSize::Small => "small.en (~466 MB)",
        }
    }

    pub fn from_index(index: u32) -> Self {
        match index {
            0 => ModelSize::Tiny,
            1 => ModelSize::Base,
            2 => ModelSize::Small,
            _ => ModelSize::Base,
        }
    }

    pub fn to_index(self) -> u32 {
        match self {
            ModelSize::Tiny => 0,
            ModelSize::Base => 1,
            ModelSize::Small => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_provider_mode")]
    pub provider_mode: ProviderMode,
    #[serde(default)]
    pub onboarding_complete: bool,
    #[serde(default)]
    pub local_model_path: Option<PathBuf>,
    #[serde(default = "default_cloud_api_base")]
    pub cloud_api_base: String,
    #[serde(default)]
    pub cloud_api_key: String,
    #[serde(default = "default_auto_copy")]
    pub auto_copy_cleaned_text: bool,
    #[serde(default = "default_auto_type")]
    pub auto_type_into_focused_app: bool,
    #[serde(default = "default_shortcut")]
    pub global_shortcut_label: String,
    #[serde(default)]
    pub model_size: ModelSize,
    #[serde(default)]
    pub input_device_name: Option<String>,
    #[serde(default)]
    pub pause_audio_during_dictation: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        let default_model = default_model_path();
        Self {
            provider_mode: default_provider_mode(),
            onboarding_complete: false,
            local_model_path: default_model.exists().then_some(default_model),
            cloud_api_base: default_cloud_api_base(),
            cloud_api_key: String::new(),
            auto_copy_cleaned_text: default_auto_copy(),
            auto_type_into_focused_app: default_auto_type(),
            global_shortcut_label: default_shortcut(),
            model_size: ModelSize::default(),
            input_device_name: None,
            pause_audio_during_dictation: false,
        }
    }
}

impl AppSettings {
    pub fn load() -> Self {
        let path = settings_path();
        let default_model = default_model_path();

        if !path.exists() {
            return Self::default();
        }

        let raw = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("Failed to read settings from {}: {err}", path.display());
                return Self::default();
            }
        };

        let mut parsed: Self = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("Failed to parse settings from {}: {err}", path.display());
                return Self::default();
            }
        };

        if parsed.local_model_path.is_none() && default_model.exists() {
            parsed.local_model_path = Some(default_model);
        }
        if parsed
            .global_shortcut_label
            .trim()
            .eq_ignore_ascii_case(LEGACY_SHORTCUT)
        {
            parsed.global_shortcut_label = default_shortcut();
            let _ = parsed.save();
        }

        parsed
    }

    pub fn mark_onboarded(&mut self) {
        self.onboarding_complete = true;
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
            set_private_permissions(parent)
                .with_context(|| format!("failed to lock down {}", parent.display()))?;
        }

        let payload = serde_json::to_string_pretty(self)?;
        fs::write(&path, payload).with_context(|| format!("failed to write {}", path.display()))?;
        set_private_permissions(&path)
            .with_context(|| format!("failed to lock down {}", path.display()))?;
        Ok(())
    }
}

pub fn settings_path() -> PathBuf {
    config_dir().join(SETTINGS_FILE_NAME)
}

pub fn config_dir() -> PathBuf {
    let mut base = dirs::config_dir().unwrap_or_else(|| PathBuf::from(Path::new(".config")));
    base.push(APP_DIR_NAME);
    base
}

pub fn data_dir() -> PathBuf {
    let mut base = dirs::data_dir().unwrap_or_else(|| PathBuf::from(Path::new(".local/share")));
    base.push(APP_DIR_NAME);
    base
}

pub fn local_models_dir() -> PathBuf {
    data_dir().join("models")
}

pub fn default_model_path() -> PathBuf {
    local_models_dir().join("ggml-base.en.bin")
}

pub fn preferred_model_path(settings: &AppSettings) -> PathBuf {
    match &settings.local_model_path {
        Some(path) => path.clone(),
        None => default_model_path(),
    }
}

pub fn model_path_for_size(size: ModelSize) -> PathBuf {
    local_models_dir().join(size.filename())
}

fn default_provider_mode() -> ProviderMode {
    ProviderMode::Local
}

fn default_cloud_api_base() -> String {
    DEFAULT_CLOUD_API_BASE.into()
}

fn default_auto_copy() -> bool {
    true
}

fn default_auto_type() -> bool {
    true
}

fn default_shortcut() -> String {
    DEFAULT_SHORTCUT.into()
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}
