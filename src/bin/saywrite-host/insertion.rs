use std::{env, process::Command};

use anyhow::{anyhow, Context, Result};

use crate::ibus;

#[derive(Debug, Clone)]
pub struct InsertionStatus {
    pub available: bool,
    pub method: String,
}

#[derive(Debug, Clone, Copy)]
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
    let method = describe_method(backend);
    InsertionStatus {
        available: !matches!(backend, Backend::Unavailable),
        method,
    }
}

pub async fn insert_text(text: &str) -> Result<String> {
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
    if ibus::gnome_wayland() {
        let context = ibus::current_input_context().ok();
        let engine = ibus::global_engine_name().ok();
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
    let mut backends = Vec::new();

    if ibus::bridge_ready() && ibus::gnome_wayland() {
        backends.push(Backend::IbusEngine);
    }
    if session.eq_ignore_ascii_case("wayland")
        && command_exists("wtype")
        && !ibus::gnome_wayland()
    {
        backends.push(Backend::Wtype);
    }
    if session.eq_ignore_ascii_case("x11") && command_exists("xdotool") {
        backends.push(Backend::Xdotool);
    }
    if command_exists("wl-copy") {
        backends.push(Backend::WlClipboard);
    }
    if command_exists("xclip") {
        backends.push(Backend::Xclip);
    }
    if command_exists("xsel") {
        backends.push(Backend::Xsel);
    }
    if command_exists("notify-send") {
        backends.push(Backend::NotifySend);
    }

    if backends.is_empty() {
        backends.push(Backend::Unavailable);
    }

    backends
}

async fn try_backend(backend: Backend, text: &str) -> Result<String> {
    match backend {
        Backend::IbusEngine => {
            ibus::commit_text(text).await?;
            Ok("Text committed through the SayWrite IBus engine.".into())
        }
        Backend::Wtype => {
            run_command("wtype", &[text]).context("Wayland text typing failed")?;
            Ok("Text typed with wtype.".into())
        }
        Backend::Xdotool => {
            run_command("xdotool", &["type", "--clearmodifiers", "--delay", "1", text])
                .context("X11 text typing failed")?;
            Ok("Text typed with xdotool.".into())
        }
        Backend::WlClipboard => {
            write_clipboard("wl-copy", &[], text)?;
            notify_transcript("Transcript copied to the clipboard. Paste it into the focused field.")?;
            Ok("Transcript copied to the Wayland clipboard.".into())
        }
        Backend::Xclip => {
            write_clipboard("xclip", &["-selection", "clipboard"], text)?;
            notify_transcript("Transcript copied to the clipboard. Paste it into the focused field.")?;
            Ok("Transcript copied to the X11 clipboard.".into())
        }
        Backend::Xsel => {
            write_clipboard("xsel", &["--clipboard", "--input"], text)?;
            notify_transcript("Transcript copied to the clipboard. Paste it into the focused field.")?;
            Ok("Transcript copied to the clipboard with xsel.".into())
        }
        Backend::NotifySend => {
            notify_transcript(text)?;
            Ok("Transcript shown as a desktop notification.".into())
        }
        Backend::Unavailable => Err(anyhow!("no insertion backend is available")),
    }
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
