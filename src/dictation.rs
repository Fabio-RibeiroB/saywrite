use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};

use crate::{
    cleanup::cleanup_transcript,
    config::{default_model_path, AppSettings, ProviderMode},
};

static ACTIVE_SESSION: OnceLock<Mutex<Option<RecordingSession>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub raw_text: String,
    pub cleaned_text: String,
}

#[derive(Debug)]
struct RecordingSession {
    child: Child,
    audio_path: PathBuf,
}

pub fn start_live(settings: &AppSettings) -> Result<String> {
    ensure_local_mode(settings)?;
    let whisper_cli = discover_whisper_cli();
    if !whisper_cli.exists() {
        return Err(anyhow!(
            "whisper.cpp CLI not found. Install the bundled runtime before starting dictation."
        ));
    }

    let mut guard = session_store().lock().expect("recording session mutex poisoned");
    if guard.is_some() {
        return Err(anyhow!("A dictation session is already running."));
    }

    let audio_path = next_recording_path()?;
    let child = Command::new("gst-launch-1.0")
        .args([
            "-q",
            "-e",
            "autoaudiosrc",
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
        ])
        .arg(format!("location={}", audio_path.display()))
        .spawn()
        .context("failed to start microphone capture")?;

    *guard = Some(RecordingSession { child, audio_path });
    Ok("Listening...".into())
}

pub fn stop_live(settings: &AppSettings) -> Result<TranscriptResult> {
    ensure_local_mode(settings)?;
    let model_path = preferred_model_path(settings);
    if !model_path.exists() {
        return Err(anyhow!(
            "No local model found. Finish setup in SayWrite before dictating."
        ));
    }

    let mut session = session_store()
        .lock()
        .expect("recording session mutex poisoned")
        .take()
        .ok_or_else(|| anyhow!("No dictation session is running."))?;

    interrupt_recording(&mut session.child)?;
    session
        .child
        .wait()
        .context("failed while stopping microphone capture")?;

    validate_recording(&session.audio_path)?;
    let raw_text = transcribe_file(&model_path, &discover_whisper_cli(), &session.audio_path)?;
    let cleaned_text = cleanup_transcript(&raw_text);
    let _ = fs::remove_file(&session.audio_path);

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

pub fn preferred_model_path(settings: &AppSettings) -> PathBuf {
    match &settings.local_model_path {
        Some(path) => path.clone(),
        None => default_model_path(),
    }
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

fn ensure_local_mode(settings: &AppSettings) -> Result<()> {
    if settings.provider_mode == ProviderMode::Cloud {
        return Err(anyhow!(
            "Cloud dictation is not wired into the Rust pipeline yet. Switch to Local."
        ));
    }
    Ok(())
}

fn next_recording_path() -> Result<PathBuf> {
    let base = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("saywrite");
    fs::create_dir_all(&base)
        .with_context(|| format!("failed to create recording directory {}", base.display()))?;

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

fn validate_recording(audio_path: &Path) -> Result<()> {
    let metadata = fs::metadata(audio_path)
        .with_context(|| format!("missing recording at {}", audio_path.display()))?;
    if metadata.len() == 0 {
        return Err(anyhow!("Microphone recording produced no audio."));
    }
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
        return Err(anyhow!(
            stderr.trim().to_string()
        ));
    }

    Ok(extract_transcript(&String::from_utf8_lossy(&output.stdout)))
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
