use std::{
    env,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::host_api;

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

const SOCKET_NAME: &str = "saywrite-host.sock";
static TOKIO_RUNTIME: OnceLock<std::result::Result<tokio::runtime::Runtime, String>> =
    OnceLock::new();

#[derive(Debug, Deserialize)]
struct HostResponse {
    ok: bool,
    status: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HostSetupStatus {
    pub binary_installed: bool,
    pub systemd_service_installed: bool,
    pub dbus_service_installed: bool,
    pub host_running: bool,
    pub install_command: String,
    pub gnome_shortcut_command: Option<String>,
}

/// Try to insert text into the focused app. Attempts D-Bus first, then Unix
/// socket, and returns an error if neither works.
pub fn send_text(text: &str, delay_seconds: f64) -> Result<String> {
    // Try D-Bus first
    if let Ok(msg) = send_text_dbus(text) {
        return Ok(msg);
    }

    // Fall back to Unix socket
    send_text_socket(text, delay_seconds)
}

/// Toggle host-side dictation. This is the primary path for app-driven
/// dictation when the host companion is available.
pub fn toggle_dictation() -> Result<String> {
    tokio_handle()?.block_on(async {
        let conn = zbus::Connection::session()
            .await
            .context("no D-Bus session bus")?;
        let proxy = zbus::Proxy::new(
            &conn,
            host_api::BUS_NAME,
            host_api::OBJECT_PATH,
            host_api::INTERFACE_NAME,
        )
        .await
        .context("failed to create D-Bus host proxy")?;

        let reply = proxy
            .call_method("ToggleDictation", &())
            .await
            .context("D-Bus toggle call failed")?
            .body()
            .context("unexpected D-Bus toggle reply format")?;

        let (ok, message): (bool, String) = reply;
        if ok {
            Ok(message)
        } else {
            Err(anyhow!("{}", message))
        }
    })
}

/// Check whether the host daemon is reachable via D-Bus or socket.
pub fn host_available() -> bool {
    host_status().is_some() || host_socket_present()
}

/// Check if the legacy Unix socket exists.
pub fn host_socket_present() -> bool {
    socket_path().is_some_and(|path| path.exists())
}

/// Check if the host D-Bus service is available on the session bus.
pub fn host_dbus_available() -> bool {
    host_status().is_some()
}

pub fn host_status() -> Option<host_api::HostStatus> {
    let handle = match tokio_handle() {
        Ok(handle) => handle,
        Err(_) => return None,
    };
    handle.block_on(async {
        let conn = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(_) => return None,
        };
        let proxy = match zbus::Proxy::new(
            &conn,
            host_api::BUS_NAME,
            host_api::OBJECT_PATH,
            host_api::INTERFACE_NAME,
        )
        .await
        {
            Ok(proxy) => proxy,
            Err(_) => return None,
        };
        let reply = proxy.call_method("GetStatus", &()).await.ok()?;
        let (status, hotkey_active, insertion_available, insertion_capability, insertion_backend): (
            String,
            bool,
            bool,
            String,
            String,
        ) = reply.body().ok()?;

        Some(host_api::HostStatus {
            status,
            hotkey_active,
            insertion_available,
            insertion_capability,
            insertion_backend,
        })
    })
}

pub fn host_setup_status() -> HostSetupStatus {
    let binary_path = host_binary_path();
    let systemd_service_path = host_systemd_service_path();
    let dbus_service_path = host_dbus_service_path();

    HostSetupStatus {
        binary_installed: binary_path.exists(),
        systemd_service_installed: systemd_service_path.exists(),
        dbus_service_installed: dbus_service_path.exists(),
        host_running: host_status().is_some(),
        install_command: host_install_command(),
        gnome_shortcut_command: gnome_shortcut_command(),
    }
}

/// Returns `true` when the SayWrite source repo is reachable and the install
/// script is present, meaning `install_host_companion()` can succeed.
/// When this returns `false` the UI should show manual-install guidance instead.
pub fn can_install_in_app() -> bool {
    repo_root()
        .map(|r| r.join("scripts/install-host.sh").exists())
        .unwrap_or(false)
}

/// Progress update sent from `install_host_companion` to the UI thread.
pub enum HostInstallUpdate {
    /// An intermediate status message to display while work is in progress.
    Progress(String),
    /// Installation completed successfully.
    Done,
}

/// Kick off host companion installation in a background thread.
/// Returns a receiver that delivers `Ok(HostInstallUpdate)` progress messages
/// or `Err(String)` on fatal failure. Channel disconnect signals the end of
/// the run (check whether the last message was `Done` or `Err`).
pub fn install_host_companion(
) -> std::sync::mpsc::Receiver<Result<HostInstallUpdate, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let repo_root = match repo_root() {
            Some(root) => root,
            None => {
                let _ = tx.send(Err(
                    "Could not locate the SayWrite source repository. \
                     Run this from the repo directory."
                        .into(),
                ));
                return;
            }
        };

        let install_script = repo_root.join("scripts/install-host.sh");
        if !install_script.exists() {
            let _ = tx.send(Err(format!(
                "Install script not found at {}",
                install_script.display()
            )));
            return;
        }

        // Build the release binary if it is not already present.
        let binary = repo_root.join("target/release/saywrite-host");
        if !binary.exists() {
            let _ = tx.send(Ok(HostInstallUpdate::Progress(
                "Building saywrite-host — this may take a minute\u{2026}".into(),
            )));
            match std::process::Command::new("cargo")
                .args(["build", "--release", "--bin", "saywrite-host"])
                .current_dir(&repo_root)
                .output()
            {
                Ok(out) if out.status.success() => {
                    let _ = tx.send(Ok(HostInstallUpdate::Progress("Build complete.".into())));
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let snippet = stderr.lines().take(6).collect::<Vec<_>>().join("\n");
                    let _ = tx.send(Err(format!("Build failed:\n{snippet}")));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("Failed to run cargo: {e}")));
                    return;
                }
            }
        }

        let _ = tx.send(Ok(HostInstallUpdate::Progress(
            "Installing host companion\u{2026}".into(),
        )));
        match std::process::Command::new("bash")
            .arg(&install_script)
            .current_dir(&repo_root)
            .output()
        {
            Ok(out) if out.status.success() => {
                let _ = tx.send(Ok(HostInstallUpdate::Done));
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                let text = if !stderr.is_empty() { stderr } else { stdout };
                let snippet = text.lines().take(6).collect::<Vec<_>>().join("\n");
                let _ = tx.send(Err(format!("Install script failed:\n{snippet}")));
            }
            Err(e) => {
                let _ = tx.send(Err(format!("Failed to run install script: {e}")));
            }
        }
    });
    rx
}

pub fn host_install_instructions() -> String {
    let setup = host_setup_status();
    let mut steps = vec![
        "Install the host companion:".to_string(),
        String::new(),
        format!("1. {}", setup.install_command),
    ];

    if let Some(command) = setup.gnome_shortcut_command.as_ref() {
        steps.push(format!("2. Optional GNOME fallback shortcut: {command}"));
    }

    steps.push(String::new());
    steps.push("After installation:".into());
    steps.push("  systemctl --user status saywrite-host".into());
    steps.push("  journalctl --user -u saywrite-host -f".into());
    steps.join("\n")
}

fn host_binary_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".local/bin/saywrite-host")
}

fn host_systemd_service_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".config/systemd/user/saywrite-host.service")
}

fn host_dbus_service_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".local/share/dbus-1/services/io.github.saywrite.Host.service")
}

fn host_install_command() -> String {
    if let Some(repo_root) = repo_root() {
        if repo_root.join("scripts/install-host.sh").exists() {
            return "cargo build --release\n   bash scripts/install-host.sh".into();
        }
    }

    "Install the native saywrite-host companion package for your distro.".into()
}

fn gnome_shortcut_command() -> Option<String> {
    if !gnome_shortcuts_supported() {
        return None;
    }

    if let Some(repo_root) = repo_root() {
        if repo_root.join("scripts/install-gnome-shortcut.sh").exists() {
            return Some("bash scripts/install-gnome-shortcut.sh".into());
        }
    }

    Some("Create a GNOME custom shortcut that runs the SayWrite host toggle command.".into())
}

fn repo_root() -> Option<PathBuf> {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let current_dir = env::current_dir().ok();

    // Prefer the executable's location — it is stable even when the user
    // launches SayWrite from an unrelated directory that happens to contain
    // its own Cargo.toml + scripts/ folder.
    [exe_dir, current_dir]
        .into_iter()
        .flatten()
        .find_map(find_repo_root)
}

fn find_repo_root(start: PathBuf) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        // Check for the SayWrite-specific install script rather than a
        // generic Cargo.toml + scripts/ pair, so we don't accidentally
        // resolve into an unrelated Rust project.
        if candidate.join("Cargo.toml").exists()
            && candidate.join("scripts/install-host.sh").exists()
        {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn gnome_shortcuts_supported() -> bool {
    env::var("XDG_CURRENT_DESKTOP")
        .map(|value| value.to_ascii_lowercase().contains("gnome"))
        .unwrap_or(false)
}

fn send_text_dbus(text: &str) -> Result<String> {
    tokio_handle()?.block_on(async {
        let conn = zbus::Connection::session()
            .await
            .context("no D-Bus session bus")?;
        let proxy = zbus::Proxy::new(
            &conn,
            host_api::BUS_NAME,
            host_api::OBJECT_PATH,
            host_api::INTERFACE_NAME,
        )
        .await
        .context("failed to create D-Bus host proxy")?;

        let reply = proxy
            .call_method("InsertText", &(text,))
            .await
            .context("D-Bus call failed")?
            .body()
            .context("unexpected D-Bus reply format")?;

        let (ok, _result_kind, message): (bool, String, String) = reply;

        if ok {
            Ok(message)
        } else {
            Err(anyhow!("{}", message))
        }
    })
}

fn send_text_socket(text: &str, delay_seconds: f64) -> Result<String> {
    let path = socket_path().ok_or_else(|| {
        anyhow!("Host integration is not running. No private runtime directory is available.")
    })?;
    if !path.exists() {
        return Err(anyhow!(
            "Host integration is not running. Text was not delivered."
        ));
    }

    let payload = serde_json::json!({
        "action": "insert_text",
        "text": text,
        "delay_seconds": delay_seconds,
    })
    .to_string();

    let mut socket = UnixStream::connect(&path).with_context(|| {
        format!(
            "failed to connect to host integration at {}",
            path.display()
        )
    })?;
    socket
        .write_all(payload.as_bytes())
        .context("failed to send insertion request")?;
    socket
        .shutdown(std::net::Shutdown::Write)
        .context("failed to finalize insertion request")?;

    let mut response = Vec::new();
    socket
        .read_to_end(&mut response)
        .context("failed to read insertion response")?;
    let message: HostResponse =
        serde_json::from_slice(&response).context("invalid host integration response")?;

    if message.ok {
        return Ok(message.status.unwrap_or_else(|| "Text delivered.".into()));
    }

    Err(anyhow!(
        "{}",
        message
            .error
            .unwrap_or_else(|| "unknown host integration error".into())
    ))
}

fn socket_path() -> Option<PathBuf> {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|dir| dir.join(SOCKET_NAME))
}

/// Subscribe to D-Bus signals from the host daemon.
/// Returns an mpsc receiver that delivers `HostEvent`s.
/// The caller should poll this with `glib::timeout_add_local`.
pub fn subscribe_host_signals() -> Option<std::sync::mpsc::Receiver<HostEvent>> {
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let handle = match tokio_handle() {
            Ok(handle) => handle,
            Err(_) => return,
        };
        handle.block_on(async {
            let conn = match zbus::Connection::session().await {
                Ok(c) => c,
                Err(_) => return,
            };
            let proxy = match zbus::Proxy::new(
                &conn,
                host_api::BUS_NAME,
                host_api::OBJECT_PATH,
                host_api::INTERFACE_NAME,
            )
            .await
            {
                Ok(p) => p,
                Err(_) => return,
            };

            use futures_util::StreamExt;
            let mut signals = match proxy.receive_all_signals().await {
                Ok(s) => s,
                Err(_) => return,
            };

            while let Some(signal) = signals.next().await {
                let header = match signal.header() {
                    Ok(h) => h,
                    Err(_) => continue,
                };
                let member = header
                    .member()
                    .ok()
                    .flatten()
                    .map(|m| m.as_str().to_string());
                match member.as_deref() {
                    Some("DictationStateChanged") => {
                        if let Ok(state) = signal.body::<String>() {
                            let _ = tx.send(HostEvent::StateChanged(state));
                        }
                    }
                    Some("TextReady") => {
                        if let Ok((cleaned, raw_text)) = signal.body::<(String, String)>() {
                            let _ = tx.send(HostEvent::TextReady { cleaned, raw_text });
                        }
                    }
                    Some("InsertionResult") => {
                        if let Ok((ok, result_kind, message)) =
                            signal.body::<(bool, String, String)>()
                        {
                            let _ = tx.send(HostEvent::InsertionResult {
                                ok,
                                result_kind,
                                message,
                            });
                        }
                    }
                    _ => {}
                }
            }
        });
    });

    Some(rx)
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
