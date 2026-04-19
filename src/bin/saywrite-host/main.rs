mod dbus;
mod input;
mod insertion;
mod service;

use std::time::Duration;

use anyhow::{Context, Result};
use tokio::signal::unix::{signal, SignalKind};

const APP_BUS_NAME: &str = "io.github.fabio.SayWrite";

#[tokio::main]
async fn main() -> Result<()> {
    if !app_session_is_active().await {
        eprintln!(
            "saywrite-host refusing to start: no process owns {APP_BUS_NAME} on the session bus"
        );
        return Ok(());
    }

    eprintln!("saywrite-host starting");
    init_ibus().await;

    let host = dbus::HostDaemon::new().context("failed to initialize host daemon")?;
    let _connection = host
        .serve()
        .await
        .context("failed to serve D-Bus host interface")?;

    tokio::spawn(async {
        if let Err(e) = input::register_and_listen().await {
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

async fn app_session_is_active() -> bool {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("saywrite-host: cannot reach session bus: {e}");
            return false;
        }
    };

    for attempt in 0..5 {
        match conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "NameHasOwner",
                &APP_BUS_NAME,
            )
            .await
            .and_then(|reply| reply.body::<bool>())
        {
            Ok(true) => return true,
            Ok(false) => {}
            Err(e) => eprintln!("saywrite-host: NameHasOwner failed: {e}"),
        }
        if attempt < 4 {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    false
}

async fn init_ibus() {
    if !input::preferred_on_this_desktop() {
        return;
    }
    match tokio::task::spawn_blocking(input::ensure_running).await {
        Ok(Ok(())) => eprintln!("IBus runtime is available"),
        Ok(Err(e)) => { eprintln!("IBus runtime unavailable: {e}"); return; }
        Err(e) => { eprintln!("IBus startup task failed: {e}"); return; }
    }
    if let Err(e) = input::ensure_bridge().await {
        eprintln!("SayWrite IBus bridge unavailable: {e}");
    } else {
        eprintln!("SayWrite IBus engine bridge is registered");
    }
    if let Ok(Ok(path)) = tokio::task::spawn_blocking(input::current_input_context).await {
        eprintln!("IBus current input context: {path}");
    }
    if let Ok(Ok(name)) = tokio::task::spawn_blocking(input::global_engine_name).await {
        eprintln!("IBus global engine: {name}");
    }
}
