use std::{
    env,
    path::{Path, PathBuf},
};

use crate::host_integration;

#[derive(Debug, Clone)]
pub struct HostSetupStatus {
    pub binary_installed: bool,
    pub systemd_service_installed: bool,
    pub dbus_service_installed: bool,
    pub host_running: bool,
    pub install_command: String,
    pub gnome_shortcut_command: Option<String>,
}

/// Returns `true` when the SayWrite source repo is reachable and the install
/// script is present, meaning `install_host_companion()` can succeed.
/// When this returns `false` the UI should show manual-install guidance instead.
pub fn can_install_in_app() -> bool {
    repo_root()
        .map(|root| root.join("scripts/install-host.sh").exists())
        .unwrap_or(false)
}

/// Progress update sent from `install_host_companion` to the UI thread.
pub enum HostInstallUpdate {
    /// An intermediate status message to display while work is in progress.
    Progress(String),
    /// Installation completed successfully.
    Done,
}

pub fn host_setup_status() -> HostSetupStatus {
    let binary_path = host_binary_path();
    let systemd_service_path = host_systemd_service_path();
    let dbus_service_path = host_dbus_service_path();

    HostSetupStatus {
        binary_installed: binary_path.exists(),
        systemd_service_installed: systemd_service_path.exists(),
        dbus_service_installed: dbus_service_path.exists(),
        host_running: host_integration::host_status().is_some(),
        install_command: host_install_command(),
        gnome_shortcut_command: gnome_shortcut_command(),
    }
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
                Err(err) => {
                    let _ = tx.send(Err(format!("Failed to run cargo: {err}")));
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
            Err(err) => {
                let _ = tx.send(Err(format!("Failed to run install script: {err}")));
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

    [exe_dir, current_dir]
        .into_iter()
        .flatten()
        .find_map(find_repo_root)
}

fn find_repo_root(start: PathBuf) -> Option<PathBuf> {
    for candidate in start.ancestors() {
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
