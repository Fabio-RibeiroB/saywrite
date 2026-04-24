use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    thread,
};

use anyhow::{anyhow, Result};
use zbus::{dbus_interface, Connection, ConnectionBuilder};

use crate::{
    input, integration_api,
    service::{DictationEvent, DictationService},
};

#[derive(Debug, Clone)]
pub enum IntegrationEvent {
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
static INTEGRATION_SERVICE: OnceLock<std::result::Result<DictationService, String>> =
    OnceLock::new();
static SUBSCRIBERS: OnceLock<Mutex<Vec<mpsc::Sender<IntegrationEvent>>>> = OnceLock::new();
static COMPATIBILITY_DBUS_CONNECTION: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();
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
            if let Err(err) = ensure_compatibility_dbus_server().await {
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
    let service = integration_service()?;
    let response = tokio_handle()?.block_on(service.insert_text(text));
    let event = IntegrationEvent::InsertionResult {
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
    let service = integration_service()?;
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

pub fn integration_available() -> bool {
    integration_status().is_some()
}

pub fn integration_status() -> Option<integration_api::IntegrationStatus> {
    let handle = tokio_handle().ok()?;
    let service = integration_service().ok()?;
    Some(handle.block_on(service.get_status()))
}

pub fn subscribe_integration_events() -> Option<mpsc::Receiver<IntegrationEvent>> {
    let (tx, rx) = mpsc::channel();
    subscribers().lock().ok()?.push(tx);
    Some(rx)
}

fn subscribers() -> &'static Mutex<Vec<mpsc::Sender<IntegrationEvent>>> {
    SUBSCRIBERS.get_or_init(|| Mutex::new(Vec::new()))
}

fn compatibility_dbus_connection() -> &'static Mutex<Option<Connection>> {
    COMPATIBILITY_DBUS_CONNECTION.get_or_init(|| Mutex::new(None))
}

fn broadcast(event: IntegrationEvent) {
    let mut guard = match subscribers().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    guard.retain(|sender| sender.send(event.clone()).is_ok());
}

fn broadcast_toggle_events(events: Vec<DictationEvent>) {
    for event in events {
        match event {
            DictationEvent::StateChanged(state) => broadcast(IntegrationEvent::StateChanged(state)),
            DictationEvent::TextReady {
                cleaned_text,
                raw_text,
            } => broadcast(IntegrationEvent::TextReady {
                cleaned: cleaned_text,
                raw_text,
            }),
            DictationEvent::InsertionResult(response) => {
                broadcast(IntegrationEvent::InsertionResult {
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
                let service = match integration_service() {
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

async fn ensure_compatibility_dbus_server() -> Result<()> {
    if compatibility_dbus_connection()
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false)
    {
        return Ok(());
    }

    let connection = ConnectionBuilder::session()?
        .name(integration_api::COMPAT_BUS_NAME)?
        .serve_at(
            integration_api::COMPAT_OBJECT_PATH,
            CompatibilityHostInterface,
        )?
        .build()
        .await?;

    if let Ok(mut guard) = compatibility_dbus_connection().lock() {
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

fn integration_service() -> Result<&'static DictationService> {
    match INTEGRATION_SERVICE.get_or_init(|| DictationService::new().map_err(|err| err.to_string()))
    {
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

struct CompatibilityHostInterface;

#[dbus_interface(name = "io.github.saywrite.Host")]
impl CompatibilityHostInterface {
    async fn get_status(&self) -> (String, bool, bool, String, String) {
        match integration_service() {
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
                integration_api::INSERTION_CAPABILITY_UNAVAILABLE.into(),
                "unavailable".into(),
            ),
        }
    }

    async fn insert_text(&self, text: &str) -> (bool, String, String) {
        match integration_service() {
            Ok(service) => {
                let response = service.insert_text(text).await;
                broadcast(IntegrationEvent::InsertionResult {
                    ok: response.ok,
                    result_kind: response.result_kind.clone(),
                    message: response.message.clone(),
                });
                (response.ok, response.result_kind, response.message)
            }
            Err(err) => (
                false,
                integration_api::INSERTION_RESULT_FAILED.into(),
                err.to_string(),
            ),
        }
    }

    async fn toggle_dictation(&self) -> (bool, String) {
        match integration_service() {
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
