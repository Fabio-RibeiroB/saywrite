use std::{
    env,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

const SOCKET_NAME: &str = "saywrite-host.sock";

#[derive(Debug, Deserialize)]
struct HostResponse {
    ok: bool,
    status: Option<String>,
    error: Option<String>,
}

pub fn send_text(text: &str, delay_seconds: f64) -> Result<String> {
    let path = socket_path();
    if !path.exists() {
        return Err(anyhow!(
            "Host integration is not running yet. SayWrite copied nothing."
        ));
    }

    let payload = serde_json::json!({
        "action": "insert_text",
        "text": text,
        "delay_seconds": delay_seconds,
    })
    .to_string();

    let mut socket = UnixStream::connect(&path)
        .with_context(|| format!("failed to connect to host integration at {}", path.display()))?;
    socket
        .write_all(payload.as_bytes())
        .context("failed to send insertion request")?;
    socket
        .shutdown(std::net::Shutdown::Write)
        .context("failed to finalize insertion request")?;

    let mut response = Vec::new();
    socket
        .read_to_end(&mut response)
        .context("failed to read insertion response")?;
    let message: HostResponse =
        serde_json::from_slice(&response).context("invalid host integration response")?;

    if message.ok {
        return Ok(message.status.unwrap_or_else(|| "Text delivered.".into()));
    }

    Err(anyhow!(
        "{}",
        message
            .error
            .unwrap_or_else(|| "unknown host integration error".into())
    ))
}

pub fn host_socket_present() -> bool {
    socket_path().exists()
}

fn socket_path() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(SOCKET_NAME)
}
