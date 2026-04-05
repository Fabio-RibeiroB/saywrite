use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use saywrite::{
    config::AppSettings,
    dictation::{self, DictationError},
    host_api::{
        HostStatus, BUS_NAME, OBJECT_PATH, STATE_DONE, STATE_IDLE, STATE_LISTENING,
        STATE_PROCESSING,
    },
};
use tokio::sync::Mutex;
use zbus::{dbus_interface, Connection, ConnectionBuilder, SignalContext};

use crate::{
    hotkey::{self, HotkeyStatus},
    insertion::{self, InsertionStatus},
};

#[derive(Debug)]
struct SharedState {
    dictation_state: String,
    last_status: String,
    last_toggle_at: Option<Instant>,
    insertion: InsertionStatus,
    connection: Option<Connection>,
}

#[derive(Clone)]
pub struct HostDaemon {
    inner: Arc<HostDaemonInner>,
}

struct HostDaemonInner {
    state: Mutex<SharedState>,
    toggle_guard: Mutex<()>,
}

struct HostInterface {
    inner: Arc<HostDaemonInner>,
}

impl HostDaemon {
    pub fn new() -> Result<Self> {
        let insertion = insertion::probe();
        let state = SharedState {
            dictation_state: STATE_IDLE.into(),
            last_status: "Host daemon initialized.".into(),
            last_toggle_at: None,
            insertion,
            connection: None,
        };

        Ok(Self {
            inner: Arc::new(HostDaemonInner {
                state: Mutex::new(state),
                toggle_guard: Mutex::new(()),
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
            let mut state = self.inner.state.lock().await;
            state.connection = Some(server.clone());
        }

        Ok(server)
    }

    pub async fn hotkey_status(&self) -> HotkeyStatus {
        hotkey::probe(&AppSettings::load())
    }
}

#[dbus_interface(name = "io.github.saywrite.Host")]
impl HostInterface {
    async fn get_status(&self) -> (String, bool, bool, String, String) {
        let hotkey = hotkey::probe(&AppSettings::load());
        let insertion = insertion::probe();
        let mut state = self.inner.state.lock().await;
        state.insertion = insertion.clone();
        let status = HostStatus {
            status: format!(
                "{} [{} via {}]",
                state.last_status, state.dictation_state, state.insertion.method
            ),
            hotkey_active: hotkey.active,
            insertion_available: state.insertion.available,
            insertion_capability: state.insertion.capability.clone(),
            insertion_backend: state.insertion.method.clone(),
        };
        (
            status.status,
            status.hotkey_active,
            status.insertion_available,
            status.insertion_capability,
            status.insertion_backend,
        )
    }

    async fn insert_text(&self, text: &str) -> (bool, String, String) {
        let result = insertion::insert_text(text).await;
        let (result_kind, message) = match &result {
            Ok(outcome) => (outcome.result_kind.clone(), outcome.message.clone()),
            Err(err) => sanitize_error(err),
        };

        {
            let mut state = self.inner.state.lock().await;
            state.last_status = message.clone();
            state.insertion = insertion::probe();
        }

        if let Some(ctxt) = self.signal_context().await {
            let _ = Self::insertion_result(&ctxt, result.is_ok(), &result_kind, &message).await;
        }
        (result.is_ok(), result_kind, message)
    }

    async fn toggle_dictation(&self) -> (bool, String) {
        let _toggle_guard = match self.inner.toggle_guard.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                let message =
                    "Dictation is already changing state. Release the shortcut and try again."
                        .to_string();
                self.transition_state_message(&message).await;
                return (false, message);
            }
        };

        if let Some(message) = self.reject_if_repeated_toggle().await {
            return (false, message);
        }

        let settings = AppSettings::load();
        let listening = {
            let state = self.inner.state.lock().await;
            state.dictation_state == STATE_LISTENING
        };

        if !listening {
            self.transition_state(STATE_PROCESSING, "Starting dictation.")
                .await;
            self.emit_state(STATE_PROCESSING).await;

            match tokio::task::spawn_blocking(move || dictation::start_live(&settings)).await {
                Ok(Ok(message)) => {
                    eprintln!("ToggleDictation start ok: {message}");
                    self.transition_state(STATE_LISTENING, &message).await;
                    self.emit_state(STATE_LISTENING).await;
                    (true, message)
                }
                Ok(Err(err)) => {
                    eprintln!("ToggleDictation start error: {err:#}");
                    let (_, message) = sanitize_error(&err);
                    self.transition_state(STATE_IDLE, &message).await;
                    self.emit_state(STATE_IDLE).await;
                    (false, message)
                }
                Err(err) => {
                    eprintln!("ToggleDictation start task error: {err}");
                    let (_, message) = sanitize_error(&anyhow::Error::new(err));
                    self.transition_state(STATE_IDLE, &message).await;
                    self.emit_state(STATE_IDLE).await;
                    (false, message)
                }
            }
        } else {
            self.transition_state(STATE_PROCESSING, "Processing transcript.")
                .await;
            self.emit_state(STATE_PROCESSING).await;

            match tokio::task::spawn_blocking(move || dictation::stop_live(&settings)).await {
                Ok(Ok(transcript)) => {
                    eprintln!(
                        "ToggleDictation stop ok: raw_len={} cleaned_len={}",
                        transcript.raw_text.len(),
                        transcript.cleaned_text.len()
                    );
                    let cleaned_text = transcript.cleaned_text.clone();
                    let raw_text = transcript.raw_text.clone();
                    if let Some(ctxt) = self.signal_context().await {
                        let _ = Self::text_ready(&ctxt, &cleaned_text, &raw_text).await;
                    }

                    let insertion_result = insertion::insert_text(&cleaned_text).await;
                    let (result_kind, insertion_message) = match &insertion_result {
                        Ok(outcome) => (outcome.result_kind.clone(), outcome.message.clone()),
                        Err(err) => sanitize_error(err),
                    };
                    eprintln!(
                        "ToggleDictation insertion result: ok={} kind={} message={}",
                        insertion_result.is_ok(),
                        result_kind,
                        insertion_message
                    );

                    self.transition_state(STATE_DONE, &insertion_message).await;
                    {
                        let mut state = self.inner.state.lock().await;
                        state.insertion = insertion::probe();
                    }

                    if let Some(ctxt) = self.signal_context().await {
                        let _ = Self::insertion_result(
                            &ctxt,
                            insertion_result.is_ok(),
                            &result_kind,
                            &insertion_message,
                        )
                        .await;
                    }
                    self.emit_state(STATE_DONE).await;
                    (insertion_result.is_ok(), insertion_message)
                }
                Ok(Err(err)) => {
                    eprintln!("ToggleDictation stop error: {err:#}");
                    let (_, message) = sanitize_error(&err);
                    self.transition_state(STATE_IDLE, &message).await;
                    self.emit_state(STATE_IDLE).await;
                    (false, message)
                }
                Err(err) => {
                    eprintln!("ToggleDictation stop task error: {err}");
                    let (_, message) = sanitize_error(&anyhow::Error::new(err));
                    self.transition_state(STATE_IDLE, &message).await;
                    self.emit_state(STATE_IDLE).await;
                    (false, message)
                }
            }
        }
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
    async fn transition_state(&self, phase: &str, message: &str) {
        let mut state = self.inner.state.lock().await;
        state.dictation_state = phase.into();
        state.last_status = message.into();
    }

    async fn transition_state_message(&self, message: &str) {
        let mut state = self.inner.state.lock().await;
        state.last_status = message.into();
    }

    async fn reject_if_repeated_toggle(&self) -> Option<String> {
        const TOGGLE_DEBOUNCE_MS: u128 = 900;

        let mut state = self.inner.state.lock().await;
        let now = Instant::now();
        if let Some(last_toggle_at) = state.last_toggle_at {
            if now.duration_since(last_toggle_at).as_millis() < TOGGLE_DEBOUNCE_MS {
                let message =
                    "Ignoring repeated shortcut activation. Release the shortcut before toggling again."
                        .to_string();
                state.last_status = message.clone();
                return Some(message);
            }
        }

        state.last_toggle_at = Some(now);
        None
    }

    async fn signal_context(&self) -> Option<SignalContext<'static>> {
        let connection = {
            let state = self.inner.state.lock().await;
            state.connection.clone()
        }?;

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
}

fn sanitize_error(err: &anyhow::Error) -> (String, String) {
    if let Some(dictation_err) = err.downcast_ref::<DictationError>() {
        let message = match dictation_err {
            DictationError::WhisperCliNotFound => {
                "whisper.cpp is not installed for the host daemon yet.".into()
            }
            DictationError::NoLocalModel => "No local model is installed yet.".into(),
            DictationError::NoAudioCaptured => "The microphone did not produce any audio.".into(),
            DictationError::MissingRuntimeDir => {
                "SayWrite could not access a private runtime directory for recordings.".into()
            }
        };
        return (saywrite::host_api::INSERTION_RESULT_FAILED.into(), message);
    }

    (
        saywrite::host_api::INSERTION_RESULT_FAILED.into(),
        "The host daemon hit an unexpected error.".into(),
    )
}
