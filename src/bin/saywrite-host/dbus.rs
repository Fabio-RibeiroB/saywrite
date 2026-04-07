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
use std::sync::OnceLock;
use tokio::sync::Mutex;
use zbus::{dbus_interface, Connection, ConnectionBuilder, SignalContext};

use crate::{
    input::{self, HotkeyStatus},
    insertion::{self, InsertionStatus},
};

struct DictationState {
    phase: String,
    last_status: String,
    last_toggle_at: Option<Instant>,
    insertion: InsertionStatus,
}

#[derive(Clone)]
pub struct HostDaemon {
    state: Arc<Mutex<DictationState>>,
    toggle_guard: Arc<Mutex<()>>,
    connection: Arc<OnceLock<Connection>>,
}

impl HostDaemon {
    pub fn new() -> Result<Self> {
        let insertion = insertion::probe();
        Ok(Self {
            state: Arc::new(Mutex::new(DictationState {
                phase: STATE_IDLE.into(),
                last_status: "Host daemon initialized.".into(),
                last_toggle_at: None,
                insertion,
            })),
            toggle_guard: Arc::new(Mutex::new(())),
            connection: Arc::new(OnceLock::new()),
        })
    }

    pub async fn serve(&self) -> Result<Connection> {
        let server = ConnectionBuilder::session()?
            .name(BUS_NAME)?
            .serve_at(OBJECT_PATH, self.clone())?
            .build()
            .await
            .context("failed to register host interface on D-Bus")?;
        self.connection.set(server.clone()).ok();
        Ok(server)
    }

    pub fn hotkey_status(&self) -> HotkeyStatus {
        input::probe(&AppSettings::load())
    }
}

#[dbus_interface(name = "io.github.saywrite.Host")]
impl HostDaemon {
    async fn get_status(&self) -> (String, bool, bool, String, String) {
        let hotkey = input::probe(&AppSettings::load());
        let insertion = insertion::probe();
        let mut state = self.state.lock().await;
        state.insertion = insertion.clone();
        let status = HostStatus {
            status: format!(
                "{} [{} via {}]",
                state.last_status, state.phase, state.insertion.method
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
            Err(err) => (saywrite::host_api::INSERTION_RESULT_FAILED.into(), error_message(err)),
        };

        {
            let mut state = self.state.lock().await;
            state.last_status = message.clone();
            state.insertion = insertion::probe();
        }

        if let Some(ctxt) = self.signal_context() {
            let _ = Self::insertion_result(&ctxt, result.is_ok(), &result_kind, &message).await;
        }
        (result.is_ok(), result_kind, message)
    }

    async fn toggle_dictation(&self) -> (bool, String) {
        let _toggle_guard = match self.toggle_guard.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                let message =
                    "Dictation is already changing state. Release the shortcut and try again."
                        .to_string();
                {
                    let mut s = self.state.lock().await;
                    s.last_status = message.clone();
                }
                return (false, message);
            }
        };

        if let Some(message) = self.reject_if_repeated_toggle().await {
            return (false, message);
        }

        let settings = AppSettings::load();
        let listening = {
            let state = self.state.lock().await;
            state.phase == STATE_LISTENING
        };

        if !listening {
            self.set_state(STATE_PROCESSING, "Starting dictation.").await;

            match tokio::task::spawn_blocking(move || dictation::start_live(&settings)).await {
                Ok(Ok(message)) => {
                    eprintln!("ToggleDictation start ok: {message}");
                    self.set_state(STATE_LISTENING, &message).await;
                    (true, message)
                }
                Ok(Err(err)) => {
                    eprintln!("ToggleDictation start error: {err:#}");
                    let message = error_message(&err);
                    self.set_state(STATE_IDLE, &message).await;
                    (false, message)
                }
                Err(err) => {
                    eprintln!("ToggleDictation start task error: {err}");
                    let message = error_message(&anyhow::Error::new(err));
                    self.set_state(STATE_IDLE, &message).await;
                    (false, message)
                }
            }
        } else {
            self.set_state(STATE_PROCESSING, "Processing transcript.").await;

            match tokio::task::spawn_blocking(move || dictation::stop_live(&settings)).await {
                Ok(Ok(transcript)) => {
                    eprintln!(
                        "ToggleDictation stop ok: raw_len={} cleaned_len={}",
                        transcript.raw_text.len(),
                        transcript.cleaned_text.len()
                    );
                    let cleaned_text = transcript.cleaned_text.clone();
                    let raw_text = transcript.raw_text.clone();
                    if let Some(ctxt) = self.signal_context() {
                        let _ = Self::text_ready(&ctxt, &cleaned_text, &raw_text).await;
                    }

                    let insertion_result = insertion::insert_text(&cleaned_text).await;
                    let (result_kind, insertion_message) = match &insertion_result {
                        Ok(outcome) => (outcome.result_kind.clone(), outcome.message.clone()),
                        Err(err) => (saywrite::host_api::INSERTION_RESULT_FAILED.into(), error_message(err)),
                    };
                    eprintln!(
                        "ToggleDictation insertion result: ok={} kind={} message={}",
                        insertion_result.is_ok(),
                        result_kind,
                        insertion_message
                    );

                    {
                        let mut state = self.state.lock().await;
                        state.insertion = insertion::probe();
                    }

                    if let Some(ctxt) = self.signal_context() {
                        let _ = Self::insertion_result(
                            &ctxt,
                            insertion_result.is_ok(),
                            &result_kind,
                            &insertion_message,
                        )
                        .await;
                    }
                    self.set_state(STATE_DONE, &insertion_message).await;
                    (insertion_result.is_ok(), insertion_message)
                }
                Ok(Err(err)) => {
                    eprintln!("ToggleDictation stop error: {err:#}");
                    let message = error_message(&err);
                    self.set_state(STATE_IDLE, &message).await;
                    (false, message)
                }
                Err(err) => {
                    eprintln!("ToggleDictation stop task error: {err}");
                    let message = error_message(&anyhow::Error::new(err));
                    self.set_state(STATE_IDLE, &message).await;
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

impl HostDaemon {
    fn signal_context(&self) -> Option<SignalContext<'static>> {
        let conn = self.connection.get()?;
        Some(SignalContext::new(conn, OBJECT_PATH).expect("valid signal context").into_owned())
    }

    async fn set_state(&self, phase: &str, message: &str) {
        {
            let mut s = self.state.lock().await;
            s.phase = phase.into();
            s.last_status = message.into();
        }
        if let Some(ctxt) = self.signal_context() {
            let _ = Self::dictation_state_changed(&ctxt, phase).await;
        }
    }

    async fn reject_if_repeated_toggle(&self) -> Option<String> {
        const TOGGLE_DEBOUNCE_MS: u128 = 900;

        let mut state = self.state.lock().await;
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
}

fn error_message(err: &anyhow::Error) -> String {
    if let Some(dictation_err) = err.downcast_ref::<DictationError>() {
        return match dictation_err {
            DictationError::WhisperCliNotFound => "whisper.cpp is not installed for the host daemon yet.".into(),
            DictationError::NoLocalModel => "No local model is installed yet.".into(),
            DictationError::NoAudioCaptured => "The microphone did not produce any audio.".into(),
            DictationError::MissingRuntimeDir => "SayWrite could not access a private runtime directory for recordings.".into(),
        };
    }
    "The host daemon hit an unexpected error.".into()
}
