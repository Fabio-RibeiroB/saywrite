use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};

use crate::host_api;

#[derive(Debug, Clone)]
pub enum HostEvent {
    StateChanged(String),
    TextReady {
        cleaned: String,
        raw_text: String,
    },
    InsertionResult {
        ok: bool,
        result_kind: String,
        message: String,
    },
}

static TOKIO_RUNTIME: OnceLock<std::result::Result<tokio::runtime::Runtime, String>> =
    OnceLock::new();

/// Insert text into the focused app via the host D-Bus interface.
pub fn send_text(text: &str) -> Result<String> {
    send_text_dbus(text)
}

/// Toggle host-side dictation. This is the primary path for app-driven
/// dictation when the host companion is available.
pub fn toggle_dictation() -> Result<String> {
    tokio_handle()?.block_on(async {
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
            .call_method("ToggleDictation", &())
            .await
            .context("D-Bus toggle call failed")?
            .body()
            .context("unexpected D-Bus toggle reply format")?;

        let (ok, message): (bool, String) = reply;
        if ok {
            Ok(message)
        } else {
            Err(anyhow!("{}", message))
        }
    })
}

/// Check whether the host daemon is reachable via D-Bus.
pub fn host_available() -> bool {
    host_status().is_some()
}

pub fn host_status() -> Option<host_api::HostStatus> {
    let handle = match tokio_handle() {
        Ok(handle) => handle,
        Err(_) => return None,
    };
    handle.block_on(async {
        let conn = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(_) => return None,
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
            Err(_) => return None,
        };
        let reply = proxy.call_method("GetStatus", &()).await.ok()?;
        let (status, hotkey_active, insertion_available, insertion_capability, insertion_backend): (
            String,
            bool,
            bool,
            String,
            String,
        ) = reply.body().ok()?;

        Some(host_api::HostStatus {
            status,
            hotkey_active,
            insertion_available,
            insertion_capability,
            insertion_backend,
        })
    })
}

fn send_text_dbus(text: &str) -> Result<String> {
    tokio_handle()?.block_on(async {
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

        let (ok, _result_kind, message): (bool, String, String) = reply;

        if ok {
            Ok(message)
        } else {
            Err(anyhow!("{}", message))
        }
    })
}


/// Subscribe to D-Bus signals from the host daemon.
/// Returns an mpsc receiver that delivers `HostEvent`s.
/// The caller should poll this with `glib::timeout_add_local`.
pub fn subscribe_host_signals() -> Option<std::sync::mpsc::Receiver<HostEvent>> {
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let handle = match tokio_handle() {
            Ok(handle) => handle,
            Err(_) => return,
        };
        handle.block_on(async {
            let conn = match zbus::Connection::session().await {
                Ok(c) => c,
                Err(_) => return,
            };
            let proxy = match zbus::Proxy::new(
                &conn,
                host_api::BUS_NAME,
                host_api::OBJECT_PATH,
                host_api::INTERFACE_NAME,
            )
            .await
            {
                Ok(p) => p,
                Err(_) => return,
            };

            use futures_util::StreamExt;
            let mut signals = match proxy.receive_all_signals().await {
                Ok(s) => s,
                Err(_) => return,
            };

            while let Some(signal) = signals.next().await {
                let header = match signal.header() {
                    Ok(h) => h,
                    Err(_) => continue,
                };
                let member = header
                    .member()
                    .ok()
                    .flatten()
                    .map(|m| m.as_str().to_string());
                match member.as_deref() {
                    Some("DictationStateChanged") => {
                        if let Ok(state) = signal.body::<String>() {
                            let _ = tx.send(HostEvent::StateChanged(state));
                        }
                    }
                    Some("TextReady") => {
                        if let Ok((cleaned, raw_text)) = signal.body::<(String, String)>() {
                            let _ = tx.send(HostEvent::TextReady { cleaned, raw_text });
                        }
                    }
                    Some("InsertionResult") => {
                        if let Ok((ok, result_kind, message)) =
                            signal.body::<(bool, String, String)>()
                        {
                            let _ = tx.send(HostEvent::InsertionResult {
                                ok,
                                result_kind,
                                message,
                            });
                        }
                    }
                    _ => {}
                }
            }
        });
    });

    Some(rx)
}

fn tokio_runtime() -> Result<&'static tokio::runtime::Runtime> {
    let runtime = TOKIO_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|err| err.to_string())
    });

    match runtime {
        Ok(rt) => Ok(rt),
        Err(err) => Err(anyhow!("failed to start tokio runtime: {err}")),
    }
}

fn tokio_handle() -> Result<&'static tokio::runtime::Handle> {
    Ok(tokio_runtime()?.handle())
}
