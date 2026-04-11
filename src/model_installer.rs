use std::{fs, path::PathBuf, process::Command, thread, time::Duration};

use anyhow::{anyhow, Context, Result};

use crate::config::{default_model_path, local_models_dir, model_path_for_size, ModelSize};

const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";
const INSTALLED_MODEL_MIN_BYTES: u64 = 1_000;
const VALID_MODEL_MIN_BYTES: u64 = 1_000_000;

/// Status returned by the installer on each progress tick.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

/// Returns true if the default model is already present on disk.
pub fn model_exists() -> bool {
    let path = default_model_path();
    path.exists()
        && fs::metadata(&path)
            .map(|m| m.len() > INSTALLED_MODEL_MIN_BYTES)
            .unwrap_or(false)
}

/// Returns true if a model of the given size is already present on disk.
pub fn model_exists_for_size(size: ModelSize) -> bool {
    let path = model_path_for_size(size);
    path.exists()
        && fs::metadata(&path)
            .map(|m| m.len() > INSTALLED_MODEL_MIN_BYTES)
            .unwrap_or(false)
}

/// Download the default whisper model to `local_models_dir()`.
pub fn download_default_model<F>(on_progress: F) -> Result<PathBuf>
where
    F: Fn(DownloadProgress),
{
    download_model(ModelSize::Base, on_progress)
}

/// Download a whisper model of the given size to `local_models_dir()`.
pub fn download_model<F>(size: ModelSize, on_progress: F) -> Result<PathBuf>
where
    F: Fn(DownloadProgress),
{
    download_model_cancellable(size, on_progress, || false)
}

/// Download a whisper model using curl, with progress polling and cancel support.
///
/// Uses curl rather than a Rust HTTP client because Hugging Face throttles
/// raw HTTP clients but not curl. Resumes partial downloads automatically.
pub fn download_model_cancellable<F, C>(
    size: ModelSize,
    on_progress: F,
    should_cancel: C,
) -> Result<PathBuf>
where
    F: Fn(DownloadProgress),
    C: Fn() -> bool,
{
    let dest = model_path_for_size(size);
    if model_exists_for_size(size) {
        return Ok(dest);
    }

    let models_dir = local_models_dir();
    fs::create_dir_all(&models_dir)
        .with_context(|| format!("failed to create {}", models_dir.display()))?;

    let part_path = dest.with_extension("bin.part");
    let url = format!("{}{}", MODEL_BASE_URL, size.filename());

    // Get total file size upfront so the UI can show a proper progress bar.
    let total_bytes = get_content_length(&url);

    // Spawn curl:
    //   -L  follow redirects (HF uses them)
    //   -C - resume from existing partial file if present
    //   --silent --show-error  no progress noise, but do surface errors
    //   --fail  exit non-zero on HTTP 4xx/5xx
    //   --retry 3  retry transient network failures
    let mut child = Command::new("curl")
        .args([
            "-L",
            "-C",
            "-",
            "--silent",
            "--show-error",
            "--fail",
            "--retry",
            "3",
            "--retry-delay",
            "2",
            "-o",
            &part_path.to_string_lossy().into_owned(),
            &url,
        ])
        .spawn()
        .context("curl is required for model downloads but was not found")?;

    // Poll the growing part file for progress every 300 ms.
    loop {
        if should_cancel() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("download canceled"));
        }

        let downloaded = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
        on_progress(DownloadProgress {
            bytes_downloaded: downloaded,
            total_bytes,
        });

        match child
            .try_wait()
            .context("failed to check curl download status")?
        {
            Some(status) if status.success() => break,
            Some(_) => {
                return Err(anyhow!(
                    "download failed — check your internet connection and try again"
                ));
            }
            None => {
                thread::sleep(Duration::from_millis(300));
            }
        }
    }

    let size_on_disk = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
    if size_on_disk < VALID_MODEL_MIN_BYTES {
        let _ = fs::remove_file(&part_path);
        return Err(anyhow!(
            "downloaded file is too small ({size_on_disk} bytes) — likely a network error"
        ));
    }

    fs::rename(&part_path, &dest).with_context(|| {
        format!(
            "failed to move {} → {}",
            part_path.display(),
            dest.display()
        )
    })?;

    Ok(dest)
}

/// Remove any partial download left behind by a failed or cancelled attempt.
pub fn cleanup_partial() {
    let part_path = default_model_path().with_extension("bin.part");
    let _ = fs::remove_file(part_path);
}

/// Remove partial download for a specific model size.
pub fn cleanup_partial_for_size(size: ModelSize) {
    let part_path = model_path_for_size(size).with_extension("bin.part");
    let _ = fs::remove_file(part_path);
}

/// Human-readable size string.
pub fn format_bytes(bytes: u64) -> String {
    if bytes < VALID_MODEL_MIN_BYTES {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    }
}

/// Fetch the Content-Length of a URL via a HEAD request using curl.
/// Returns None if unavailable.
fn get_content_length(url: &str) -> Option<u64> {
    let output = Command::new("curl")
        .args(["-sI", "--max-time", "10", url])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.to_ascii_lowercase().starts_with("content-length:") {
            return line.split(':').nth(1)?.trim().parse().ok();
        }
    }
    None
}
