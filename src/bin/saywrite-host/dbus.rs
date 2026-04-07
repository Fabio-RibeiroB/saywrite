use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::Mutex;
use zbus::{dbus_interface, Connection, ConnectionBuilder, SignalContext};

use saywrite::host_api::{BUS_NAME, OBJECT_PATH};

use crate::{
    hotkey::HotkeyStatus,
    service::{HostService, HostSignalEvent, InsertResponse},
};

#[derive(Clone)]
pub struct HostDaemon {
    inner: Arc<HostDaemonInner>,
}

struct HostDaemonInner {
    connection: Mutex<Option<Connection>>,
    service: HostService,
}

struct HostInterface {
    inner: Arc<HostDaemonInner>,
}

impl HostDaemon {
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: Arc::new(HostDaemonInner {
                connection: Mutex::new(None),
                service: HostService::new()?,
            }),
        })
    }

    pub async fn serve(&self) -> Result<Connection> {
        let server = ConnectionBuilder::session()?
            .name(BUS_NAME)?
            .serve_at(
                OBJECT_PATH,
                HostInterface {
                    inner: self.inner.clone(),
                },
            )?
            .build()
            .await
            .context("failed to register host interface on D-Bus")?;

        {
            let mut connection = self.inner.connection.lock().await;
            *connection = Some(server.clone());
        }

        Ok(server)
    }

    pub async fn hotkey_status(&self) -> HotkeyStatus {
        self.inner.service.hotkey_status().await
    }
}

#[dbus_interface(name = "io.github.saywrite.Host")]
impl HostInterface {
    async fn get_status(&self) -> (String, bool, bool, String, String) {
        let status = self.inner.service.get_status().await;
        (
            status.status,
            status.hotkey_active,
            status.insertion_available,
            status.insertion_capability,
            status.insertion_backend,
        )
    }

    async fn insert_text(&self, text: &str) -> (bool, String, String) {
        let result = self.inner.service.insert_text(text).await;
        self.emit_insertion_result(&result).await;
        (result.ok, result.result_kind, result.message)
    }

    async fn toggle_dictation(&self) -> (bool, String) {
        let result = self.inner.service.toggle_dictation().await;
        for event in &result.events {
            match event {
                HostSignalEvent::StateChanged(phase) => self.emit_state(phase).await,
                HostSignalEvent::TextReady {
                    cleaned_text,
                    raw_text,
                } => self.emit_text_ready(cleaned_text, raw_text).await,
                HostSignalEvent::InsertionResult(response) => {
                    self.emit_insertion_result(response).await
                }
            }
        }
        (result.ok, result.message)
    }

    #[dbus_interface(signal)]
    async fn dictation_state_changed(ctxt: &SignalContext<'_>, state: &str) -> zbus::Result<()>;

    #[dbus_interface(signal)]
    async fn text_ready(
        ctxt: &SignalContext<'_>,
        cleaned_text: &str,
        raw_text: &str,
    ) -> zbus::Result<()>;

    #[dbus_interface(signal)]
    async fn insertion_result(
        ctxt: &SignalContext<'_>,
        ok: bool,
        result_kind: &str,
        message: &str,
    ) -> zbus::Result<()>;
}

impl HostInterface {
    async fn signal_context(&self) -> Option<SignalContext<'static>> {
        let connection = { self.inner.connection.lock().await.clone() }?;

        Some(
            SignalContext::new(&connection, OBJECT_PATH)
                .expect("valid signal context")
                .into_owned(),
        )
    }

    async fn emit_state(&self, phase: &str) {
        if let Some(ctxt) = self.signal_context().await {
            let _ = Self::dictation_state_changed(&ctxt, phase).await;
        }
    }

    async fn emit_text_ready(&self, cleaned_text: &str, raw_text: &str) {
        if let Some(ctxt) = self.signal_context().await {
            let _ = Self::text_ready(&ctxt, cleaned_text, raw_text).await;
        }
    }

    async fn emit_insertion_result(&self, response: &InsertResponse) {
        if let Some(ctxt) = self.signal_context().await {
            let _ = Self::insertion_result(
                &ctxt,
                response.ok,
                &response.result_kind,
                &response.message,
            )
            .await;
        }
    }
}
