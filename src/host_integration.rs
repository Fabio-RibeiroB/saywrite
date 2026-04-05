use std::{
    env,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::host_api;

const SOCKET_NAME: &str = "saywrite-host.sock";

#[derive(Debug, Deserialize)]
struct HostResponse {
    ok: bool,
    status: Option<String>,
    error: Option<String>,
}

/// Try to insert text into the focused app. Attempts D-Bus first, then Unix
/// socket, and returns an error if neither works.
pub fn send_text(text: &str, delay_seconds: f64) -> Result<String> {
    // Try D-Bus first
    if let Ok(msg) = send_text_dbus(text) {
        return Ok(msg);
    }

    // Fall back to Unix socket
    send_text_socket(text, delay_seconds)
}

/// Check whether the host daemon is reachable via D-Bus or socket.
pub fn host_available() -> bool {
    host_dbus_available() || host_socket_present()
}

/// Check if the legacy Unix socket exists.
pub fn host_socket_present() -> bool {
    socket_path().exists()
}

/// Check if the host D-Bus service is available on the session bus.
pub fn host_dbus_available() -> bool {
    // Blocking probe: try to call GetStatus with a short timeout.
    // If the bus name isn't registered, this returns quickly.
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return false,
    };
    rt.block_on(async {
        let conn = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(_) => return false,
        };
        let proxy = match zbus::Proxy::new(
            &conn,
            host_api::BUS_NAME,
            host_api::OBJECT_PATH,
            host_api::INTERFACE_NAME,
        )
        .await
        {
            Ok(proxy) => proxy,
            Err(_) => return false,
        };
        proxy.call_method("GetStatus", &()).await.is_ok()
    })
}

fn send_text_dbus(text: &str) -> Result<String> {
    let rt = tokio::runtime::Runtime::new().context("failed to start tokio runtime")?;
    rt.block_on(async {
        let conn = zbus::Connection::session()
            .await
            .context("no D-Bus session bus")?;
        let proxy = zbus::Proxy::new(
            &conn,
            host_api::BUS_NAME,
            host_api::OBJECT_PATH,
            host_api::INTERFACE_NAME,
        )
        .await
        .context("failed to create D-Bus host proxy")?;

        let reply = proxy
            .call_method("InsertText", &(text,))
            .await
            .context("D-Bus call failed")?
            .body()
            .context("unexpected D-Bus reply format")?;

        let (ok, message): (bool, String) = reply;

        if ok {
            Ok(message)
        } else {
            Err(anyhow!("{}", message))
        }
    })
}

fn send_text_socket(text: &str, delay_seconds: f64) -> Result<String> {
    let path = socket_path();
    if !path.exists() {
        return Err(anyhow!(
            "Host integration is not running. Text was not delivered."
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

fn socket_path() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(SOCKET_NAME)
}
