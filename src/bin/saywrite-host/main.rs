mod dbus;
mod hotkey;
mod insertion;

use anyhow::{Context, Result};
use tokio::signal::unix::{signal, SignalKind};

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("saywrite-host starting");

    let host = dbus::HostDaemon::new().context("failed to initialize host daemon")?;
    let _connection = host
        .serve()
        .await
        .context("failed to serve D-Bus host interface")?;

    let hotkey_status = host.hotkey_status().await;
    eprintln!("saywrite-host ready: {}", hotkey_status.message);
    if !hotkey_status.active {
        eprintln!("{}", hotkey_status.setup_hint);
    }

    let mut sigterm = signal(SignalKind::terminate()).context("failed to listen for SIGTERM")?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
    }

    eprintln!("saywrite-host shutting down");
    Ok(())
}
