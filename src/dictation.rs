use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};

use crate::{
    cleanup::cleanup_transcript,
    config::{preferred_model_path, AppSettings, ProviderMode},
};

static ACTIVE_SESSION: OnceLock<Mutex<Option<RecordingSession>>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInputDevice {
    pub id: String,
    pub label: String,
}
/// Base GStreamer capture pipeline. The source element and any properties
/// are prepended at launch time by `build_capture_args()`.
const GST_CAPTURE_PIPELINE_TAIL: &[&str] = &[
    "!",
    "audioconvert",
    "!",
    "audioresample",
    "!",
    "audio/x-raw,rate=16000,channels=1",
    "!",
    "wavenc",
    "!",
    "filesink",
];

/// Build the full gst-launch argument list, choosing the best microphone
/// source available on this system. If `device_override` is set, use that
/// device directly. Otherwise, we query WirePlumber for the default source
/// and avoid `autoaudiosrc` because it can pick up monitor/loopback sources.
pub fn build_capture_args(location: &str, device_override: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec!["-q".into(), "-e".into()];

    let device = device_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(default_pipewire_source);

    args.push("pulsesrc".into());
    if let Some(name) = device {
        args.push(format!("device={name}"));
    }

    for part in GST_CAPTURE_PIPELINE_TAIL {
        args.push((*part).into());
    }
    args.push(format!("location={location}"));
    args
}

/// Ask WirePlumber for the default audio source node name.
/// Returns `None` if wpctl is unavailable or output is unparseable.
fn default_pipewire_source() -> Option<String> {
    let output = Command::new("wpctl")
        .args(["inspect", "@DEFAULT_AUDIO_SOURCE@"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // Look for: node.name = "alsa_input.usb-...-00.mono-fallback"
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("node.name") {
            if let Some(value) = line.split('=').nth(1) {
                let name = value.trim().trim_matches('"');
                // Skip monitor sources — they capture desktop audio, not mic
                if !name.contains("monitor") {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub raw_text: String,
    pub cleaned_text: String,
}

#[derive(Debug)]
pub enum DictationError {
    WhisperCliNotFound,
    NoLocalModel,
    NoAudioCaptured,
    MissingRuntimeDir,
}

impl std::fmt::Display for DictationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DictationError::WhisperCliNotFound => {
                write!(
                    f,
                    "whisper.cpp CLI not found. Install the bundled runtime before starting dictation."
                )
            }
            DictationError::NoLocalModel => {
                write!(
                    f,
                    "No local model found. Finish setup in SayWrite before dictating."
                )
            }
            DictationError::NoAudioCaptured => {
                write!(f, "Microphone recording produced no audio.")
            }
            DictationError::MissingRuntimeDir => {
                write!(
                    f,
                    "XDG_RUNTIME_DIR is not set. SayWrite needs a private runtime directory for recordings."
                )
            }
        }
    }
}

impl std::error::Error for DictationError {}

#[derive(Debug)]
struct RecordingSession {
    child: Child,
    audio_path: PathBuf,
}

struct RecordingFileGuard {
    path: PathBuf,
}

impl RecordingFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RecordingFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn start_live(settings: &AppSettings) -> Result<String> {
    if settings.provider_mode == ProviderMode::Local {
        let whisper_cli = discover_whisper_cli();
        if !whisper_cli.exists() {
            return Err(anyhow::Error::new(DictationError::WhisperCliNotFound));
        }
    }

    let mut guard = session_store()
        .lock()
        .expect("recording session mutex poisoned");
    if guard.is_some() {
        return Err(anyhow!("A dictation session is already running."));
    }

    if settings.pause_audio_during_dictation {
        mute_playback(true);
    }

    let audio_path = next_recording_path()?;
    let capture_args = build_capture_args(
        &audio_path.display().to_string(),
        settings.input_device_name.as_deref(),
    );
    let child = Command::new("gst-launch-1.0")
        .args(&capture_args)
        .spawn()
        .context("failed to start microphone capture")?;

    *guard = Some(RecordingSession { child, audio_path });
    Ok("Listening...".into())
}

pub fn stop_live(settings: &AppSettings) -> Result<TranscriptResult> {
    let mut session = session_store()
        .lock()
        .expect("recording session mutex poisoned")
        .take()
        .ok_or_else(|| anyhow!("No dictation session is running."))?;
    let audio_file = RecordingFileGuard::new(session.audio_path.clone());

    if settings.pause_audio_during_dictation {
        mute_playback(false);
    }

    interrupt_recording(&mut session.child)?;
    session
        .child
        .wait()
        .context("failed while stopping microphone capture")?;

    validate_recording(audio_file.path())?;
    let raw_text = match settings.provider_mode {
        ProviderMode::Cloud => transcribe_cloud(settings, audio_file.path())?,
        ProviderMode::Local => {
            let model_path = preferred_model_path(settings);
            if !model_path.exists() {
                return Err(anyhow::Error::new(DictationError::NoLocalModel));
            }
            transcribe_file(&model_path, &discover_whisper_cli(), audio_file.path())?
        }
    };
    let cleaned_text = cleanup_transcript(&raw_text);

    Ok(TranscriptResult {
        raw_text,
        cleaned_text,
    })
}

pub fn discover_whisper_cli() -> PathBuf {
    if let Ok(path) = env::var("SAYWRITE_WHISPER_CLI") {
        return PathBuf::from(path);
    }

    // Flatpak installs to /app/bin; check there first, then dev paths.
    let candidates = [
        PathBuf::from("/app/bin/whisper-cli"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/whisper.cpp/build/bin/whisper-cli"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/whisper.cpp/build/bin/main"),
    ];

    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| PathBuf::from("whisper-cli"))
}

pub fn active_session() -> bool {
    session_store()
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false)
}

fn session_store() -> &'static Mutex<Option<RecordingSession>> {
    ACTIVE_SESSION.get_or_init(|| Mutex::new(None))
}

fn transcribe_cloud(settings: &AppSettings, audio_path: &Path) -> Result<String> {
    if settings.cloud_api_key.is_empty() {
        return Err(anyhow!(
            "Cloud API key is not set. Configure it in Settings."
        ));
    }

    let audio_data = fs::read(audio_path)
        .with_context(|| format!("failed to read audio at {}", audio_path.display()))?;

    let boundary = format!(
        "----SayWrite{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );

    let mut body = Vec::new();
    // file field
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
    body.extend_from_slice(&audio_data);
    body.extend_from_slice(b"\r\n");
    // model field
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n"
        )
        .as_bytes(),
    );
    // language field
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"language\"\r\n\r\nen\r\n")
            .as_bytes(),
    );
    // closing boundary
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let url = format!(
        "{}/audio/transcriptions",
        settings.cloud_api_base.trim_end_matches('/')
    );

    let response = ureq::post(&url)
        .set(
            "Authorization",
            &format!("Bearer {}", settings.cloud_api_key),
        )
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send_bytes(&body)
        .map_err(|e| anyhow!("Cloud transcription request failed: {e}"))?;

    let response_text = response
        .into_string()
        .context("failed to read cloud transcription response")?;
    let json: serde_json::Value = serde_json::from_str(&response_text)
        .context("failed to parse cloud transcription response")?;

    json["text"]
        .as_str()
        .map(|s: &str| s.to_string())
        .ok_or_else(|| anyhow!("Cloud response missing 'text' field"))
}

fn next_recording_path() -> Result<PathBuf> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::Error::new(DictationError::MissingRuntimeDir))?;
    let base = runtime_dir.join("saywrite");
    fs::create_dir_all(&base)
        .with_context(|| format!("failed to create recording directory {}", base.display()))?;
    set_private_dir_permissions(&base)
        .with_context(|| format!("failed to secure recording directory {}", base.display()))?;

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?
        .as_millis();
    Ok(base.join(format!("dictation-{millis}.wav")))
}

fn interrupt_recording(child: &mut Child) -> Result<()> {
    let pid = child.id().to_string();
    let status = Command::new("kill")
        .args(["-INT", &pid])
        .status()
        .context("failed to signal microphone recorder")?;
    if !status.success() {
        return Err(anyhow!("microphone recorder did not accept stop signal"));
    }
    Ok(())
}

/// Minimum RMS energy (0.0–1.0 scale for 16-bit PCM) below which we treat
/// the recording as silence. This filters out electrical crosstalk and
/// background hum that whisper would otherwise hallucinate on.
const SILENCE_RMS_THRESHOLD: f64 = 0.0001;

fn validate_recording(audio_path: &Path) -> Result<()> {
    let metadata = fs::metadata(audio_path)
        .with_context(|| format!("missing recording at {}", audio_path.display()))?;
    if metadata.len() == 0 {
        return Err(anyhow::Error::new(DictationError::NoAudioCaptured));
    }

    // Read WAV samples and check RMS energy. The pipeline produces 16-bit
    // mono PCM at 16 kHz wrapped in a WAV container (44-byte header).
    if let Ok(data) = fs::read(audio_path) {
        if data.len() > 44 {
            let samples = &data[44..];
            let count = samples.len() / 2;
            if count > 0 {
                let sum_sq: f64 = samples
                    .chunks_exact(2)
                    .map(|pair| {
                        let sample = i16::from_le_bytes([pair[0], pair[1]]) as f64 / 32768.0;
                        sample * sample
                    })
                    .sum();
                let rms = (sum_sq / count as f64).sqrt();
                if rms < SILENCE_RMS_THRESHOLD {
                    return Err(anyhow::Error::new(DictationError::NoAudioCaptured));
                }
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn transcribe_file(model_path: &Path, whisper_cli: &Path, audio_path: &Path) -> Result<String> {
    let output = Command::new(whisper_cli)
        .args([
            "--file",
            &audio_path.display().to_string(),
            "--model",
            &model_path.display().to_string(),
            "--language",
            "en",
            "--no-timestamps",
        ])
        .output()
        .with_context(|| format!("failed to start {}", whisper_cli.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(stderr.trim().to_string()));
    }

    Ok(extract_transcript(&String::from_utf8_lossy(&output.stdout)))
}

/// Mute or unmute the default audio sink via WirePlumber.
/// Failures are logged but never fatal — audio control is best-effort.
fn mute_playback(mute: bool) {
    let value = if mute { "1" } else { "0" };
    match Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SINK@", value])
        .status()
    {
        Ok(status) if !status.success() => {
            eprintln!("wpctl set-mute exited with {status}");
        }
        Err(err) => {
            eprintln!("failed to run wpctl set-mute: {err}");
        }
        _ => {}
    }
}

/// List available audio input devices via `pactl`.
pub fn list_input_devices() -> Vec<AudioInputDevice> {
    let output = match Command::new("pactl")
        .args(["list", "short", "sources"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let id = parts[1].to_string();
            // Skip monitor sources — they capture desktop audio, not mic
            if id.contains(".monitor") {
                continue;
            }
            let label = id
                .replace("alsa_input.", "")
                .replace(['_', '.'], " ");
            devices.push(AudioInputDevice { id, label });
        }
    }
    devices
}

fn extract_transcript(stdout: &str) -> String {
    let mut lines = Vec::new();
    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("whisper_")
            || line.starts_with("system_info:")
            || line.starts_with("main:")
        {
            continue;
        }
        if let Some((_, content)) = line.split_once(']') {
            if line.starts_with('[') {
                let content = content.trim();
                if !content.is_empty() {
                    lines.push(content.to_string());
                }
                continue;
            }
        }
        lines.push(line.to_string());
    }

    lines.join(" ").trim().to_string()
}
