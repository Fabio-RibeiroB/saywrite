use std::{sync::Arc, time::Instant};

use anyhow::Result;
use saywrite::{
    config::AppSettings,
    dictation::{self, DictationError},
    host_api::{
        HostStatus, INSERTION_RESULT_FAILED, STATE_DONE, STATE_IDLE, STATE_LISTENING,
        STATE_PROCESSING,
    },
};
use tokio::sync::Mutex;

use crate::{
    input::{self, HotkeyStatus},
    insertion::{self, InsertionStatus},
};

#[derive(Debug)]
struct SharedState {
    dictation_state: String,
    last_status: String,
    last_toggle_at: Option<Instant>,
    insertion: InsertionStatus,
}

#[derive(Clone)]
pub struct HostService {
    inner: Arc<HostServiceInner>,
}

struct HostServiceInner {
    state: Mutex<SharedState>,
    toggle_guard: Mutex<()>,
}

#[derive(Debug, Clone)]
pub struct InsertResponse {
    pub ok: bool,
    pub result_kind: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum HostSignalEvent {
    StateChanged(String),
    TextReady { cleaned_text: String, raw_text: String },
    InsertionResult(InsertResponse),
}

#[derive(Debug, Clone)]
pub struct ToggleResponse {
    pub ok: bool,
    pub message: String,
    pub events: Vec<HostSignalEvent>,
}

impl HostService {
    pub fn new() -> Result<Self> {
        let state = SharedState {
            dictation_state: STATE_IDLE.into(),
            last_status: "Host daemon initialized.".into(),
            last_toggle_at: None,
            insertion: insertion::probe(),
        };

        Ok(Self {
            inner: Arc::new(HostServiceInner {
                state: Mutex::new(state),
                toggle_guard: Mutex::new(()),
            }),
        })
    }

    pub async fn hotkey_status(&self) -> HotkeyStatus {
        input::probe(&AppSettings::load())
    }

    pub async fn get_status(&self) -> HostStatus {
        let hotkey = input::probe(&AppSettings::load());
        let insertion = insertion::probe();
        let mut state = self.inner.state.lock().await;
        state.insertion = insertion.clone();

        HostStatus {
            status: format!(
                "{} [{} via {}]",
                state.last_status, state.dictation_state, state.insertion.method
            ),
            hotkey_active: hotkey.active,
            insertion_available: state.insertion.available,
            insertion_capability: state.insertion.capability.clone(),
            insertion_backend: state.insertion.method.clone(),
        }
    }

    pub async fn insert_text(&self, text: &str) -> InsertResponse {
        let result = insertion::insert_text(text).await;
        let (result_kind, message) = match &result {
            Ok(outcome) => (outcome.result_kind.clone(), outcome.message.clone()),
            Err(err) => sanitize_error(err),
        };

        let mut state = self.inner.state.lock().await;
        state.last_status = message.clone();
        state.insertion = insertion::probe();

        InsertResponse {
            ok: result.is_ok(),
            result_kind,
            message,
        }
    }

    pub async fn toggle_dictation(&self) -> ToggleResponse {
        let _toggle_guard = match self.inner.toggle_guard.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                let message =
                    "Dictation is already changing state. Release the shortcut and try again."
                        .to_string();
                self.transition_state_message(&message).await;
                return ToggleResponse {
                    ok: false,
                    message,
                    events: Vec::new(),
                };
            }
        };

        if let Some(message) = self.reject_if_repeated_toggle().await {
            return ToggleResponse {
                ok: false,
                message,
                events: Vec::new(),
            };
        }

        let settings = AppSettings::load();
        let listening = {
            let state = self.inner.state.lock().await;
            state.dictation_state == STATE_LISTENING
        };

        if !listening {
            self.transition_state(STATE_PROCESSING, "Starting dictation.")
                .await;

            match tokio::task::spawn_blocking(move || dictation::start_live(&settings)).await {
                Ok(Ok(message)) => {
                    eprintln!("ToggleDictation start ok: {message}");
                    self.transition_state(STATE_LISTENING, &message).await;
                    ToggleResponse {
                        ok: true,
                        message,
                        events: vec![
                            HostSignalEvent::StateChanged(STATE_PROCESSING.into()),
                            HostSignalEvent::StateChanged(STATE_LISTENING.into()),
                        ],
                    }
                }
                Ok(Err(err)) => {
                    eprintln!("ToggleDictation start error: {err:#}");
                    let (_, message) = sanitize_error(&err);
                    self.transition_state(STATE_IDLE, &message).await;
                    ToggleResponse {
                        ok: false,
                        message: message.clone(),
                        events: vec![
                            HostSignalEvent::StateChanged(STATE_PROCESSING.into()),
                            HostSignalEvent::StateChanged(STATE_IDLE.into()),
                        ],
                    }
                }
                Err(err) => {
                    eprintln!("ToggleDictation start task error: {err}");
                    let (_, message) = sanitize_error(&anyhow::Error::new(err));
                    self.transition_state(STATE_IDLE, &message).await;
                    ToggleResponse {
                        ok: false,
                        message: message.clone(),
                        events: vec![
                            HostSignalEvent::StateChanged(STATE_PROCESSING.into()),
                            HostSignalEvent::StateChanged(STATE_IDLE.into()),
                        ],
                    }
                }
            }
        } else {
            self.transition_state(STATE_PROCESSING, "Processing transcript.")
                .await;

            match tokio::task::spawn_blocking(move || dictation::stop_live(&settings)).await {
                Ok(Ok(transcript)) => {
                    eprintln!(
                        "ToggleDictation stop ok: raw_len={} cleaned_len={}",
                        transcript.raw_text.len(),
                        transcript.cleaned_text.len()
                    );
                    let cleaned_text = transcript.cleaned_text.clone();
                    let raw_text = transcript.raw_text.clone();

                    let insertion_result = self.insert_text(&cleaned_text).await;
                    eprintln!(
                        "ToggleDictation insertion result: ok={} kind={} message={}",
                        insertion_result.ok, insertion_result.result_kind, insertion_result.message
                    );

                    self.transition_state(STATE_DONE, &insertion_result.message)
                        .await;
                    ToggleResponse {
                        ok: insertion_result.ok,
                        message: insertion_result.message.clone(),
                        events: vec![
                            HostSignalEvent::StateChanged(STATE_PROCESSING.into()),
                            HostSignalEvent::TextReady {
                                cleaned_text,
                                raw_text,
                            },
                            HostSignalEvent::InsertionResult(insertion_result),
                            HostSignalEvent::StateChanged(STATE_DONE.into()),
                        ],
                    }
                }
                Ok(Err(err)) => {
                    eprintln!("ToggleDictation stop error: {err:#}");
                    let (_, message) = sanitize_error(&err);
                    self.transition_state(STATE_IDLE, &message).await;
                    ToggleResponse {
                        ok: false,
                        message: message.clone(),
                        events: vec![
                            HostSignalEvent::StateChanged(STATE_PROCESSING.into()),
                            HostSignalEvent::StateChanged(STATE_IDLE.into()),
                        ],
                    }
                }
                Err(err) => {
                    eprintln!("ToggleDictation stop task error: {err}");
                    let (_, message) = sanitize_error(&anyhow::Error::new(err));
                    self.transition_state(STATE_IDLE, &message).await;
                    ToggleResponse {
                        ok: false,
                        message: message.clone(),
                        events: vec![
                            HostSignalEvent::StateChanged(STATE_PROCESSING.into()),
                            HostSignalEvent::StateChanged(STATE_IDLE.into()),
                        ],
                    }
                }
            }
        }
    }

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
        return (INSERTION_RESULT_FAILED.into(), message);
    }

    (
        INSERTION_RESULT_FAILED.into(),
        "The host daemon hit an unexpected error.".into(),
    )
}
