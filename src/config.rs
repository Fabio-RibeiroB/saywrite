use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};

const APP_DIR_NAME: &str = "saywrite";
const SETTINGS_FILE_NAME: &str = "settings.json";
const DEFAULT_CLOUD_API_BASE: &str = "https://api.openai.com/v1";
const DEFAULT_SHORTCUT: &str = "Super+Alt+D";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderMode {
    Local,
    Cloud,
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
            Err(_) => return Self::default(),
        };

        let mut parsed: Self = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(_) => return Self::default(),
        };

        if parsed.local_model_path.is_none() && default_model.exists() {
            parsed.local_model_path = Some(default_model);
        }

        parsed
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let payload = serde_json::to_string_pretty(self)?;
        fs::write(&path, payload)
            .with_context(|| format!("failed to write {}", path.display()))?;
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
    let mut base = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from(Path::new(".local/share")));
    base.push(APP_DIR_NAME);
    base
}

pub fn local_models_dir() -> PathBuf {
    data_dir().join("models")
}

pub fn default_model_path() -> PathBuf {
    local_models_dir().join("ggml-base.en.bin")
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
