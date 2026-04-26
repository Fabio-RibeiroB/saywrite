use std::{sync::Arc, time::Instant};

use crate::{
    config::AppSettings,
    dictation::{self, DictationError},
    integration_api::{
        IntegrationStatus, INSERTION_RESULT_FAILED, STATE_DONE, STATE_IDLE, STATE_LISTENING,
        STATE_PROCESSING,
    },
};
use anyhow::Result;
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
pub struct DictationService {
    inner: Arc<DictationServiceInner>,
}

struct DictationServiceInner {
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
pub enum DictationEvent {
    StateChanged(String),
    TextReady {
        cleaned_text: String,
        raw_text: String,
    },
    InsertionResult(InsertResponse),
}

#[derive(Debug, Clone)]
pub struct ToggleResponse {
    pub ok: bool,
    pub message: String,
    pub events: Vec<DictationEvent>,
}

impl DictationService {
    pub fn new() -> Result<Self> {
        let state = SharedState {
            dictation_state: STATE_IDLE.into(),
            last_status: "Native integration initialized.".into(),
            last_toggle_at: None,
            insertion: insertion::probe(),
        };

        Ok(Self {
            inner: Arc::new(DictationServiceInner {
                state: Mutex::new(state),
                toggle_guard: Mutex::new(()),
            }),
        })
    }

    pub async fn hotkey_status(&self) -> HotkeyStatus {
        input::probe(&AppSettings::load())
    }

    pub async fn get_status(&self) -> IntegrationStatus {
        let hotkey = input::probe(&AppSettings::load());
        let insertion = insertion::probe();
        let mut state = self.inner.state.lock().await;
        state.insertion = insertion.clone();

        IntegrationStatus {
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
                            DictationEvent::StateChanged(STATE_PROCESSING.into()),
                            DictationEvent::StateChanged(STATE_LISTENING.into()),
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
                            DictationEvent::StateChanged(STATE_PROCESSING.into()),
                            DictationEvent::StateChanged(STATE_IDLE.into()),
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
                            DictationEvent::StateChanged(STATE_PROCESSING.into()),
                            DictationEvent::StateChanged(STATE_IDLE.into()),
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
                            DictationEvent::StateChanged(STATE_PROCESSING.into()),
                            DictationEvent::TextReady {
                                cleaned_text,
                                raw_text,
                            },
                            DictationEvent::InsertionResult(insertion_result),
                            DictationEvent::StateChanged(STATE_DONE.into()),
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
                            DictationEvent::StateChanged(STATE_PROCESSING.into()),
                            DictationEvent::StateChanged(STATE_IDLE.into()),
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
                            DictationEvent::StateChanged(STATE_PROCESSING.into()),
                            DictationEvent::StateChanged(STATE_IDLE.into()),
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

    pub(crate) async fn reject_if_repeated_toggle(&self) -> Option<String> {
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

pub(crate) fn sanitize_error(err: &anyhow::Error) -> (String, String) {
    if let Some(dictation_err) = err.downcast_ref::<DictationError>() {
        let message = match dictation_err {
            DictationError::WhisperCliNotFound => "whisper.cpp is not installed yet.".into(),
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
        "SayWrite hit an unexpected error.".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::DictationError;

    #[test]
    fn sanitize_whisper_not_found() {
        let (kind, msg) = sanitize_error(&anyhow::Error::new(DictationError::WhisperCliNotFound));
        assert_eq!(kind, INSERTION_RESULT_FAILED);
        assert!(
            msg.contains("whisper"),
            "expected whisper mention, got: {msg}"
        );
    }

    #[test]
    fn sanitize_no_local_model() {
        let (kind, msg) = sanitize_error(&anyhow::Error::new(DictationError::NoLocalModel));
        assert_eq!(kind, INSERTION_RESULT_FAILED);
        assert!(msg.contains("model"), "expected model mention, got: {msg}");
    }

    #[test]
    fn sanitize_no_audio_captured() {
        let (kind, msg) = sanitize_error(&anyhow::Error::new(DictationError::NoAudioCaptured));
        assert_eq!(kind, INSERTION_RESULT_FAILED);
        assert!(
            msg.contains("microphone") || msg.contains("audio"),
            "expected audio/microphone mention, got: {msg}"
        );
    }

    #[test]
    fn sanitize_missing_runtime_dir() {
        let (kind, msg) = sanitize_error(&anyhow::Error::new(DictationError::MissingRuntimeDir));
        assert_eq!(kind, INSERTION_RESULT_FAILED);
        assert!(
            msg.contains("directory") || msg.contains("runtime"),
            "expected directory/runtime mention, got: {msg}"
        );
    }

    #[test]
    fn sanitize_generic_error_yields_fallback_message() {
        let (kind, msg) = sanitize_error(&anyhow::anyhow!("some opaque internal failure"));
        assert_eq!(kind, INSERTION_RESULT_FAILED);
        assert!(
            msg.contains("unexpected"),
            "expected fallback message, got: {msg}"
        );
    }

    #[test]
    fn all_sanitized_errors_use_failed_result_kind() {
        let errors: Vec<anyhow::Error> = vec![
            anyhow::Error::new(DictationError::WhisperCliNotFound),
            anyhow::Error::new(DictationError::NoLocalModel),
            anyhow::Error::new(DictationError::NoAudioCaptured),
            anyhow::Error::new(DictationError::MissingRuntimeDir),
            anyhow::anyhow!("generic"),
        ];
        for err in errors {
            let (kind, _) = sanitize_error(&err);
            assert_eq!(
                kind, INSERTION_RESULT_FAILED,
                "all errors must map to failed result kind"
            );
        }
    }

    #[tokio::test]
    async fn debounce_passes_first_toggle_and_rejects_immediate_repeat() {
        let service = DictationService::new().expect("DictationService::new");

        let first = service.reject_if_repeated_toggle().await;
        assert!(
            first.is_none(),
            "first toggle should not be debounced, got: {first:?}"
        );

        let second = service.reject_if_repeated_toggle().await;
        assert!(second.is_some(), "immediate repeat should be debounced");
        let msg = second.unwrap();
        assert!(
            msg.contains("shortcut") || msg.contains("repeated"),
            "rejection message should explain the debounce, got: {msg}"
        );
    }

    #[tokio::test]
    async fn debounce_allows_toggle_after_cooldown() {
        use std::time::Duration;

        let service = DictationService::new().expect("DictationService::new");

        service.reject_if_repeated_toggle().await;
        // Wait out the 900ms debounce window
        tokio::time::sleep(Duration::from_millis(950)).await;

        let after_cooldown = service.reject_if_repeated_toggle().await;
        assert!(
            after_cooldown.is_none(),
            "toggle after cooldown should be allowed, got: {after_cooldown:?}"
        );
    }
}
