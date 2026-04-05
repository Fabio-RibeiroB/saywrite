use std::{env, process::Command};

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone)]
pub struct InsertionStatus {
    pub available: bool,
    pub method: String,
}

#[derive(Debug, Clone, Copy)]
enum Backend {
    Wtype,
    Xdotool,
    WlClipboard,
    Xclip,
    Xsel,
    Unavailable,
}

pub fn probe() -> InsertionStatus {
    let backend = detect_backend();
    InsertionStatus {
        available: !matches!(backend, Backend::Unavailable),
        method: backend_name(backend).into(),
    }
}

pub fn insert_text(text: &str) -> Result<String> {
    if text.trim().is_empty() {
        return Err(anyhow!("No text was provided for insertion."));
    }

    match detect_backend() {
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
            Ok("Text copied to the Wayland clipboard.".into())
        }
        Backend::Xclip => {
            write_clipboard("xclip", &["-selection", "clipboard"], text)?;
            Ok("Text copied to the X11 clipboard.".into())
        }
        Backend::Xsel => {
            write_clipboard("xsel", &["--clipboard", "--input"], text)?;
            Ok("Text copied to the clipboard with xsel.".into())
        }
        Backend::Unavailable => Err(anyhow!(
            "No supported insertion backend found. Install wtype, wl-copy, xdotool, xclip, or xsel."
        )),
    }
}

fn detect_backend() -> Backend {
    let session = env::var("XDG_SESSION_TYPE").unwrap_or_default();

    if session.eq_ignore_ascii_case("wayland") && command_exists("wtype") {
        return Backend::Wtype;
    }
    if session.eq_ignore_ascii_case("x11") && command_exists("xdotool") {
        return Backend::Xdotool;
    }
    if command_exists("wl-copy") {
        return Backend::WlClipboard;
    }
    if command_exists("xclip") {
        return Backend::Xclip;
    }
    if command_exists("xsel") {
        return Backend::Xsel;
    }

    Backend::Unavailable
}

fn backend_name(backend: Backend) -> &'static str {
    match backend {
        Backend::Wtype => "wtype",
        Backend::Xdotool => "xdotool",
        Backend::WlClipboard => "wl-copy",
        Backend::Xclip => "xclip",
        Backend::Xsel => "xsel",
        Backend::Unavailable => "unavailable",
    }
}

fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .args(["-lc", &format!("command -v {} >/dev/null 2>&1", command)])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
