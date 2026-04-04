use std::{
    path::PathBuf,
    process::Command,
};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub raw_text: String,
    pub cleaned_text: String,
}

#[derive(Debug, Deserialize)]
struct TranscribeResponse {
    ok: bool,
    raw_text: Option<String>,
    cleaned_text: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SendTextResponse {
    ok: bool,
    status: Option<String>,
    error: Option<String>,
}

pub fn start_live() -> Result<String> {
    let output = run_python(&["-m", "saywrite.bridge_cli", "start-live"])?;
    let response: SendTextResponse =
        serde_json::from_slice(&output.stdout).context("invalid live-start response")?;
    if !response.ok {
        return Err(anyhow!(
            "{}",
            response.error.unwrap_or_else(|| "failed to start dictation".into())
        ));
    }
    Ok(response.status.unwrap_or_else(|| "Listening...".into()))
}

pub fn stop_live() -> Result<TranscriptResult> {
    let output = run_python(&["-m", "saywrite.bridge_cli", "stop-live"])?;
    let response: TranscribeResponse =
        serde_json::from_slice(&output.stdout).context("invalid live-stop response")?;
    if !response.ok {
        return Err(anyhow!(
            "{}",
            response.error.unwrap_or_else(|| "failed to stop dictation".into())
        ));
    }
    Ok(TranscriptResult {
        raw_text: response.raw_text.unwrap_or_default(),
        cleaned_text: response.cleaned_text.unwrap_or_default(),
    })
}

pub fn send_text(text: &str, delay_seconds: f64) -> Result<String> {
    let output = run_python(&[
        "-m",
        "saywrite.bridge_cli",
        "send-text",
        "--text",
        text,
        "--delay-seconds",
        &delay_seconds.to_string(),
    ])?;
    let response: SendTextResponse =
        serde_json::from_slice(&output.stdout).context("invalid host-helper response")?;
    if !response.ok {
        return Err(anyhow!(
            "{}",
            response.error.unwrap_or_else(|| "host helper request failed".into())
        ));
    }
    Ok(response
        .status
        .unwrap_or_else(|| "Text delivered.".into()))
}

fn run_python(args: &[&str]) -> Result<std::process::Output> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("python3")
        .args(args)
        .current_dir(repo_root)
        .output()
        .context("failed to start python bridge")?;

    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(stderr.trim().to_string()));
    }

    Ok(output)
}
