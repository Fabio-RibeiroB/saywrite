use std::{env, process::Command};

use anyhow::{anyhow, Context, Result};
use saywrite::host_api;

use crate::input;

#[derive(Debug, Clone)]
pub struct InsertionStatus {
    pub available: bool,
    pub capability: String,
    pub method: String,
}

#[derive(Debug, Clone)]
pub struct InsertionOutcome {
    pub result_kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    IbusEngine,
    Wtype,
    Xdotool,
    WlClipboard,
    Xclip,
    Xsel,
    NotifySend,
    Unavailable,
}

pub fn probe() -> InsertionStatus {
    let backend = detect_backend();
    InsertionStatus {
        available: !matches!(backend, Backend::Unavailable),
        capability: capability_for_backend(backend).into(),
        method: describe_method(backend),
    }
}

pub async fn insert_text(text: &str) -> Result<InsertionOutcome> {
    if text.trim().is_empty() {
        return Err(anyhow!("No text was provided for insertion."));
    }

    let mut failures = Vec::new();
    for backend in candidate_backends() {
        match try_backend(backend, text).await {
            Ok(message) => return Ok(message),
            Err(err) => failures.push(format!("{}: {}", backend_name(backend), err)),
        }
    }

    if failures.is_empty() {
        Err(anyhow!(
            "No supported insertion backend found. Install wtype, wl-copy, xdotool, xclip, or xsel."
        ))
    } else {
        Err(anyhow!(
            "All insertion backends failed: {}",
            failures.join(" | ")
        ))
    }
}

fn capability_for_backend(backend: Backend) -> &'static str {
    match backend {
        Backend::IbusEngine | Backend::Wtype | Backend::Xdotool => {
            host_api::INSERTION_CAPABILITY_TYPING
        }
        Backend::WlClipboard | Backend::Xclip | Backend::Xsel => {
            host_api::INSERTION_CAPABILITY_CLIPBOARD_ONLY
        }
        Backend::NotifySend => host_api::INSERTION_CAPABILITY_NOTIFICATION_ONLY,
        Backend::Unavailable => host_api::INSERTION_CAPABILITY_UNAVAILABLE,
    }
}

fn result_kind_for_backend(backend: Backend) -> &'static str {
    match backend {
        Backend::IbusEngine | Backend::Wtype | Backend::Xdotool => host_api::INSERTION_RESULT_TYPED,
        Backend::WlClipboard | Backend::Xclip | Backend::Xsel => host_api::INSERTION_RESULT_COPIED,
        Backend::NotifySend => host_api::INSERTION_RESULT_NOTIFIED,
        Backend::Unavailable => host_api::INSERTION_RESULT_FAILED,
    }
}

fn detect_backend() -> Backend {
    candidate_backends()
        .into_iter()
        .next()
        .unwrap_or(Backend::Unavailable)
}

fn backend_name(backend: Backend) -> &'static str {
    match backend {
        Backend::IbusEngine => "ibus-engine",
        Backend::Wtype => "wtype",
        Backend::Xdotool => "xdotool",
        Backend::WlClipboard => "wl-copy",
        Backend::Xclip => "xclip",
        Backend::Xsel => "xsel",
        Backend::NotifySend => "notify-send",
        Backend::Unavailable => "unavailable",
    }
}

fn describe_method(backend: Backend) -> String {
    if input::gnome_wayland() {
        let context = input::current_input_context().ok();
        let engine = input::global_engine_name().ok();
        match (context, engine) {
            (Some(context), Some(engine)) => {
                return format!("{}; ibus:{} @ {}", backend_name(backend), engine, context);
            }
            (Some(context), None) => {
                return format!("{}; ibus-context:{}", backend_name(backend), context);
            }
            (None, Some(engine)) => {
                return format!("{}; ibus:{}", backend_name(backend), engine);
            }
            (None, None) => {}
        }
    }

    backend_name(backend).into()
}

fn command_exists(command: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| {
            env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(command);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

fn candidate_backends() -> Vec<Backend> {
    let session = env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let mut available = Vec::new();

    if input::bridge_ready() {
        available.push(Backend::IbusEngine);
    }
    if command_exists("wtype") {
        available.push(Backend::Wtype);
    }
    if command_exists("xdotool") {
        available.push(Backend::Xdotool);
    }
    if command_exists("wl-copy") {
        available.push(Backend::WlClipboard);
    }
    if command_exists("xclip") {
        available.push(Backend::Xclip);
    }
    if command_exists("xsel") {
        available.push(Backend::Xsel);
    }
    if command_exists("notify-send") {
        available.push(Backend::NotifySend);
    }

    candidate_backends_for(
        &session,
        input::gnome_wayland(),
        input::bridge_ready(),
        &available,
    )
}

fn candidate_backends_for(
    session: &str,
    gnome_wayland: bool,
    bridge_ready: bool,
    available: &[Backend],
) -> Vec<Backend> {
    let mut backends = Vec::new();

    if bridge_ready && gnome_wayland && available.contains(&Backend::IbusEngine) {
        backends.push(Backend::IbusEngine);
    }
    if session.eq_ignore_ascii_case("wayland")
        && !gnome_wayland
        && available.contains(&Backend::Wtype)
    {
        backends.push(Backend::Wtype);
    }
    if session.eq_ignore_ascii_case("x11") && available.contains(&Backend::Xdotool) {
        backends.push(Backend::Xdotool);
    }
    if available.contains(&Backend::WlClipboard) {
        backends.push(Backend::WlClipboard);
    }
    if available.contains(&Backend::Xclip) {
        backends.push(Backend::Xclip);
    }
    if available.contains(&Backend::Xsel) {
        backends.push(Backend::Xsel);
    }
    if available.contains(&Backend::NotifySend) {
        backends.push(Backend::NotifySend);
    }

    if backends.is_empty() {
        backends.push(Backend::Unavailable);
    }

    backends
}

async fn try_backend(backend: Backend, text: &str) -> Result<InsertionOutcome> {
    match backend {
        Backend::IbusEngine => {
            input::commit_text(text).await?;
            Ok(InsertionOutcome {
                result_kind: result_kind_for_backend(backend).into(),
                message: "Text committed through the SayWrite IBus engine.".into(),
            })
        }
        Backend::Wtype => {
            run_command("wtype", &[text]).context("Wayland text typing failed")?;
            Ok(InsertionOutcome {
                result_kind: result_kind_for_backend(backend).into(),
                message: "Text typed with wtype.".into(),
            })
        }
        Backend::Xdotool => {
            run_command(
                "xdotool",
                &["type", "--clearmodifiers", "--delay", "1", text],
            )
            .context("X11 text typing failed")?;
            Ok(InsertionOutcome {
                result_kind: result_kind_for_backend(backend).into(),
                message: "Text typed with xdotool.".into(),
            })
        }
        Backend::WlClipboard => clipboard_outcome(backend, "wl-copy", &[], text),
        Backend::Xclip => clipboard_outcome(backend, "xclip", &["-selection", "clipboard"], text),
        Backend::Xsel => clipboard_outcome(backend, "xsel", &["--clipboard", "--input"], text),
        Backend::NotifySend => {
            notify_transcript(text)?;
            Ok(InsertionOutcome {
                result_kind: result_kind_for_backend(backend).into(),
                message: "Transcript shown as a desktop notification.".into(),
            })
        }
        Backend::Unavailable => Err(anyhow!("no insertion backend is available")),
    }
}

fn clipboard_outcome(backend: Backend, tool: &str, args: &[&str], text: &str) -> Result<InsertionOutcome> {
    write_clipboard(tool, args, text)?;
    let _ = notify_transcript("Transcript copied to the clipboard. Paste it into the focused field.");
    Ok(InsertionOutcome {
        result_kind: result_kind_for_backend(backend).into(),
        message: format!("Transcript copied to the clipboard via {tool}."),
    })
}

fn run_command(command: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(command)
        .args(args)
        .status()
        .with_context(|| format!("failed to start {}", command))?;
    if status.success() {
        return Ok(());
    }
    Err(anyhow!("{} exited with status {}", command, status))
}

fn write_clipboard(command: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start {}", command))?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(text.as_bytes())
            .with_context(|| format!("failed to write to {}", command))?;
    }

    let status = child
        .wait()
        .with_context(|| format!("failed waiting for {}", command))?;
    if status.success() {
        return Ok(());
    }
    Err(anyhow!("{} exited with status {}", command, status))
}

fn notify_transcript(text: &str) -> Result<()> {
    let preview = if text.chars().count() > 300 {
        let truncated: String = text.chars().take(300).collect();
        format!("{truncated}...")
    } else {
        text.to_string()
    };

    run_command(
        "notify-send",
        &[
            "--app-name=SayWrite",
            "--expire-time=10000",
            "SayWrite Transcript",
            &preview,
        ],
    )
    .context("desktop notification failed")
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_backends_for, capability_for_backend, result_kind_for_backend, Backend,
        host_api,
    };

    #[test]
    fn classifies_typing_backends_honestly() {
        assert_eq!(
            capability_for_backend(Backend::IbusEngine),
            host_api::INSERTION_CAPABILITY_TYPING
        );
        assert_eq!(
            capability_for_backend(Backend::Wtype),
            host_api::INSERTION_CAPABILITY_TYPING
        );
        assert_eq!(
            capability_for_backend(Backend::Xdotool),
            host_api::INSERTION_CAPABILITY_TYPING
        );
    }

    #[test]
    fn classifies_clipboard_and_notification_backends_separately() {
        assert_eq!(
            capability_for_backend(Backend::WlClipboard),
            host_api::INSERTION_CAPABILITY_CLIPBOARD_ONLY
        );
        assert_eq!(
            capability_for_backend(Backend::NotifySend),
            host_api::INSERTION_CAPABILITY_NOTIFICATION_ONLY
        );
        assert_eq!(
            capability_for_backend(Backend::Unavailable),
            host_api::INSERTION_CAPABILITY_UNAVAILABLE
        );
    }

    #[test]
    fn reports_result_kind_for_each_backend_honestly() {
        assert_eq!(
            result_kind_for_backend(Backend::IbusEngine),
            host_api::INSERTION_RESULT_TYPED
        );
        assert_eq!(
            result_kind_for_backend(Backend::Wtype),
            host_api::INSERTION_RESULT_TYPED
        );
        assert_eq!(
            result_kind_for_backend(Backend::Xdotool),
            host_api::INSERTION_RESULT_TYPED
        );
        assert_eq!(
            result_kind_for_backend(Backend::WlClipboard),
            host_api::INSERTION_RESULT_COPIED
        );
        assert_eq!(
            result_kind_for_backend(Backend::Xclip),
            host_api::INSERTION_RESULT_COPIED
        );
        assert_eq!(
            result_kind_for_backend(Backend::Xsel),
            host_api::INSERTION_RESULT_COPIED
        );
        assert_eq!(
            result_kind_for_backend(Backend::NotifySend),
            host_api::INSERTION_RESULT_NOTIFIED
        );
        assert_eq!(
            result_kind_for_backend(Backend::Unavailable),
            host_api::INSERTION_RESULT_FAILED
        );
    }

    #[test]
    fn prefers_ibus_on_gnome_wayland() {
        let backends = candidate_backends_for(
            "wayland",
            true,
            true,
            &[
                Backend::Wtype,
                Backend::WlClipboard,
                Backend::NotifySend,
                Backend::IbusEngine,
            ],
        );
        assert_eq!(backends.first().copied(), Some(Backend::IbusEngine));
    }

    #[test]
    fn prefers_typing_backend_on_x11() {
        let backends = candidate_backends_for(
            "x11",
            false,
            false,
            &[Backend::Xclip, Backend::Xdotool, Backend::NotifySend],
        );
        assert_eq!(backends.first().copied(), Some(Backend::Xdotool));
    }

    #[test]
    fn falls_back_to_clipboard_on_unknown_session_type() {
        let backends = candidate_backends_for(
            "tty",
            false,
            false,
            &[Backend::Wtype, Backend::Xdotool, Backend::Xclip, Backend::NotifySend],
        );
        assert_eq!(
            backends,
            vec![Backend::Xclip, Backend::NotifySend]
        );
    }

    #[test]
    fn returns_unavailable_when_no_backends_exist() {
        let backends = candidate_backends_for("wayland", false, false, &[]);
        assert_eq!(backends, vec![Backend::Unavailable]);
    }

    #[test]
    fn prefers_wtype_on_non_gnome_wayland_even_if_ibus_exists() {
        let backends = candidate_backends_for(
            "wayland",
            false,
            true,
            &[Backend::IbusEngine, Backend::Wtype, Backend::WlClipboard],
        );
        assert_eq!(
            backends,
            vec![Backend::Wtype, Backend::WlClipboard]
        );
    }
}
