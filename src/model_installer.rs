use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

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
///
/// Calls `on_progress` periodically with the current download state.
/// Performs an atomic write: downloads to a `.part` file first, then renames.
pub fn download_default_model<F>(on_progress: F) -> Result<PathBuf>
where
    F: Fn(DownloadProgress),
{
    download_model(ModelSize::Base, on_progress)
}

/// Download a whisper model of the given size to `local_models_dir()`.
///
/// Supports download resume: if a `.part` file exists, sends a Range header
/// to continue from where it left off.
pub fn download_model<F>(size: ModelSize, on_progress: F) -> Result<PathBuf>
where
    F: Fn(DownloadProgress),
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

    // Check for existing partial download
    let existing_size = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);

    let response = if existing_size > 0 {
        // Try resume
        let resp = ureq::get(&url)
            .set("Range", &format!("bytes={}-", existing_size))
            .call()
            .map_err(|e| anyhow!("failed to download model: {e}"))?;
        if resp.status() == 206 {
            // Partial content — resume
            resp
        } else {
            // Server doesn't support range; restart
            let _ = fs::remove_file(&part_path);
            ureq::get(&url)
                .call()
                .map_err(|e| anyhow!("failed to download model: {e}"))?
        }
    } else {
        ureq::get(&url)
            .call()
            .map_err(|e| anyhow!("failed to download model: {e}"))?
    };

    let is_resume = response.status() == 206;
    let content_length = response
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok());
    let total_bytes = content_length.map(|cl| if is_resume { cl + existing_size } else { cl });

    let mut reader = response.into_reader();
    let mut file = if is_resume {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&part_path)
            .with_context(|| format!("failed to open {} for resume", part_path.display()))?;
        f.seek(SeekFrom::End(0))?;
        f
    } else {
        fs::File::create(&part_path)
            .with_context(|| format!("failed to create {}", part_path.display()))?
    };

    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = if is_resume { existing_size } else { 0 };

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

/// Remove any partial download left behind by a failed attempt.
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
