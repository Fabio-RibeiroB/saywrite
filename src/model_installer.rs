use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};

use anyhow::{anyhow, Context, Result};

use crate::config::{default_model_path, local_models_dir};

const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";

/// Status returned by the installer on each progress tick.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

/// Returns true if the default model is already present on disk.
pub fn model_exists() -> bool {
    let path = default_model_path();
    path.exists() && fs::metadata(&path).map(|m| m.len() > 1_000).unwrap_or(false)
}

/// Download the default whisper model to `local_models_dir()`.
///
/// Calls `on_progress` periodically with the current download state.
/// Performs an atomic write: downloads to a `.part` file first, then renames.
pub fn download_default_model<F>(on_progress: F) -> Result<PathBuf>
where
    F: Fn(DownloadProgress),
{
    let dest = default_model_path();
    if model_exists() {
        return Ok(dest);
    }

    let models_dir = local_models_dir();
    fs::create_dir_all(&models_dir)
        .with_context(|| format!("failed to create {}", models_dir.display()))?;

    let part_path = dest.with_extension("bin.part");

    let response = ureq::get(MODEL_URL)
        .call()
        .map_err(|e| anyhow!("failed to download model: {e}"))?;

    let total_bytes = response
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok());

    let mut reader = response.into_reader();
    let mut file = fs::File::create(&part_path)
        .with_context(|| format!("failed to create {}", part_path.display()))?;

    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;

    loop {
        let n = reader.read(&mut buf).context("download interrupted")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .context("failed to write model data")?;
        downloaded += n as u64;
        on_progress(DownloadProgress {
            bytes_downloaded: downloaded,
            total_bytes,
        });
    }

    file.flush().context("failed to flush model file")?;
    drop(file);

    // Validate: file shouldn't be tiny (corrupt/empty)
    let size = fs::metadata(&part_path)
        .map(|m| m.len())
        .unwrap_or(0);
    if size < 1_000_000 {
        let _ = fs::remove_file(&part_path);
        return Err(anyhow!(
            "downloaded file is too small ({size} bytes) — likely a network error"
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

/// Remove any partial download left behind by a failed attempt.
pub fn cleanup_partial() {
    let part_path = default_model_path().with_extension("bin.part");
    let _ = fs::remove_file(part_path);
}

/// Human-readable size string.
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1_000_000 {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    }
}
