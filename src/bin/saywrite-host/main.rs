mod dbus;
mod hotkey;
mod ibus;
mod insertion;

use anyhow::{Context, Result};
use tokio::signal::unix::{signal, SignalKind};

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("saywrite-host starting");

    if ibus::preferred_on_this_desktop() {
        match tokio::task::spawn_blocking(ibus::ensure_running).await {
            Ok(Ok(())) => {
                eprintln!("IBus runtime is available");
                match ibus::ensure_bridge().await {
                    Ok(()) => eprintln!("SayWrite IBus engine bridge is registered"),
                    Err(err) => eprintln!("SayWrite IBus bridge unavailable: {err}"),
                }
                match tokio::task::spawn_blocking(ibus::current_input_context).await {
                    Ok(Ok(path)) => eprintln!("IBus current input context: {path}"),
                    Ok(Err(err)) => eprintln!("IBus current input context unavailable: {err}"),
                    Err(err) => eprintln!("IBus current input context task failed: {err}"),
                }
                match tokio::task::spawn_blocking(ibus::global_engine_name).await {
                    Ok(Ok(name)) => eprintln!("IBus global engine: {name}"),
                    Ok(Err(err)) => eprintln!("IBus global engine unavailable: {err}"),
                    Err(err) => eprintln!("IBus global engine task failed: {err}"),
                }
            }
            Ok(Err(err)) => eprintln!("IBus runtime unavailable: {err}"),
            Err(err) => eprintln!("IBus startup task failed: {err}"),
        }
    }

    let host = dbus::HostDaemon::new().context("failed to initialize host daemon")?;
    let _connection = host
        .serve()
        .await
        .context("failed to serve D-Bus host interface")?;

    // Spawn global shortcut listener (non-fatal if portal unavailable)
    tokio::spawn(async {
        if let Err(e) = hotkey::register_and_listen().await {
            eprintln!("Global shortcut registration failed: {e}");
        }
    });

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
