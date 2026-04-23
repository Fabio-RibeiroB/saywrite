use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
        Arc, Mutex, OnceLock,
    },
    thread,
};

use anyhow::{anyhow, Result};
use zbus::{dbus_interface, Connection, ConnectionBuilder};

use crate::{
    host_api,
    input,
    service::{HostService, HostSignalEvent},
};

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
static HOST_SERVICE: OnceLock<std::result::Result<HostService, String>> = OnceLock::new();
static SUBSCRIBERS: OnceLock<Mutex<Vec<mpsc::Sender<HostEvent>>>> = OnceLock::new();
static DBUS_CONNECTION: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();
static BACKGROUND_STARTED: AtomicBool = AtomicBool::new(false);

pub fn start_background_integration() {
    install_toggle_handler();

    if BACKGROUND_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::spawn(|| {
        let handle = match tokio_handle() {
            Ok(handle) => handle.clone(),
            Err(err) => {
                eprintln!("SayWrite: failed to start runtime for native integration: {err}");
                return;
            }
        };

        handle.block_on(async {
            init_ibus().await;
            if let Err(err) = ensure_dbus_server().await {
                eprintln!("SayWrite: failed to expose compatibility D-Bus interface: {err}");
            }
            if let Err(err) = input::register_and_listen().await {
                eprintln!("SayWrite: global shortcut registration failed: {err}");
            }
        });
    });
}

pub fn restart_shortcut_listener() {
    install_toggle_handler();
}

pub fn send_text(text: &str) -> Result<String> {
    let service = host_service()?;
    let response = tokio_handle()?.block_on(service.insert_text(text));
    let event = HostEvent::InsertionResult {
        ok: response.ok,
        result_kind: response.result_kind.clone(),
        message: response.message.clone(),
    };
    broadcast(event);

    if response.ok {
        Ok(response.message)
    } else {
        Err(anyhow!("{}", response.message))
    }
}

pub fn toggle_dictation() -> Result<String> {
    let service = host_service()?;
    let response = tokio_handle()?.block_on(service.toggle_dictation());
    let message = response.message.clone();
    let ok = response.ok;
    broadcast_toggle_events(response.events);

    if ok {
        Ok(message)
    } else {
        Err(anyhow!("{}", message))
    }
}

pub fn host_available() -> bool {
    host_status().is_some()
}

pub fn host_status() -> Option<host_api::HostStatus> {
    let handle = tokio_handle().ok()?;
    let service = host_service().ok()?;
    Some(handle.block_on(service.get_status()))
}

pub fn subscribe_host_signals() -> Option<mpsc::Receiver<HostEvent>> {
    let (tx, rx) = mpsc::channel();
    subscribers().lock().ok()?.push(tx);
    Some(rx)
}

fn subscribers() -> &'static Mutex<Vec<mpsc::Sender<HostEvent>>> {
    SUBSCRIBERS.get_or_init(|| Mutex::new(Vec::new()))
}

fn dbus_connection_holder() -> &'static Mutex<Option<Connection>> {
    DBUS_CONNECTION.get_or_init(|| Mutex::new(None))
}

fn broadcast(event: HostEvent) {
    let mut guard = match subscribers().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    guard.retain(|sender| sender.send(event.clone()).is_ok());
}

fn broadcast_toggle_events(events: Vec<HostSignalEvent>) {
    for event in events {
        match event {
            HostSignalEvent::StateChanged(state) => broadcast(HostEvent::StateChanged(state)),
            HostSignalEvent::TextReady {
                cleaned_text,
                raw_text,
            } => broadcast(HostEvent::TextReady {
                cleaned: cleaned_text,
                raw_text,
            }),
            HostSignalEvent::InsertionResult(response) => {
                broadcast(HostEvent::InsertionResult {
                    ok: response.ok,
                    result_kind: response.result_kind,
                    message: response.message,
                });
            }
        }
    }
}

fn install_toggle_handler() {
    input::set_toggle_handler(Arc::new(|| {
        if let Ok(handle) = tokio_handle() {
            let handle = handle.clone();
            handle.spawn(async {
                let service = match host_service() {
                    Ok(service) => service.clone(),
                    Err(err) => {
                        eprintln!("SayWrite: native toggle handler unavailable: {err}");
                        return;
                    }
                };

                let response = service.toggle_dictation().await;
                broadcast_toggle_events(response.events);
            });
        }
    }));
}

async fn ensure_dbus_server() -> Result<()> {
    if dbus_connection_holder()
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false)
    {
        return Ok(());
    }

    let connection = ConnectionBuilder::session()?
        .name(host_api::BUS_NAME)?
        .serve_at(host_api::OBJECT_PATH, AppHostInterface)?
        .build()
        .await?;

    if let Ok(mut guard) = dbus_connection_holder().lock() {
        *guard = Some(connection);
    }

    Ok(())
}

async fn init_ibus() {
    if !input::preferred_on_this_desktop() {
        return;
    }

    match tokio::task::spawn_blocking(input::ensure_running).await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            eprintln!("SayWrite: IBus runtime unavailable: {err}");
            return;
        }
        Err(err) => {
            eprintln!("SayWrite: IBus startup task failed: {err}");
            return;
        }
    }

    if let Err(err) = input::ensure_bridge().await {
        eprintln!("SayWrite: IBus bridge unavailable: {err}");
    }
}

fn host_service() -> Result<&'static HostService> {
    match HOST_SERVICE.get_or_init(|| HostService::new().map_err(|err| err.to_string())) {
        Ok(service) => Ok(service),
        Err(err) => Err(anyhow!("failed to initialize native integration: {err}")),
    }
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

struct AppHostInterface;

#[dbus_interface(name = "io.github.saywrite.Host")]
impl AppHostInterface {
    async fn get_status(&self) -> (String, bool, bool, String, String) {
        match host_service() {
            Ok(service) => {
                let status = service.get_status().await;
                (
                    status.status,
                    status.hotkey_active,
                    status.insertion_available,
                    status.insertion_capability,
                    status.insertion_backend,
                )
            }
            Err(err) => (
                err.to_string(),
                false,
                false,
                host_api::INSERTION_CAPABILITY_UNAVAILABLE.into(),
                "unavailable".into(),
            ),
        }
    }

    async fn insert_text(&self, text: &str) -> (bool, String, String) {
        match host_service() {
            Ok(service) => {
                let response = service.insert_text(text).await;
                broadcast(HostEvent::InsertionResult {
                    ok: response.ok,
                    result_kind: response.result_kind.clone(),
                    message: response.message.clone(),
                });
                (response.ok, response.result_kind, response.message)
            }
            Err(err) => (
                false,
                host_api::INSERTION_RESULT_FAILED.into(),
                err.to_string(),
            ),
        }
    }

    async fn toggle_dictation(&self) -> (bool, String) {
        match host_service() {
            Ok(service) => {
                let response = service.toggle_dictation().await;
                let message = response.message.clone();
                let ok = response.ok;
                broadcast_toggle_events(response.events);
                (ok, message)
            }
            Err(err) => (false, err.to_string()),
        }
    }
}
