use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
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

pub fn can_install_in_app() -> bool {
    install_script_path().is_some()
}

pub enum HostInstallUpdate {
    Progress(String),
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

pub fn install_host_companion() -> std::sync::mpsc::Receiver<Result<HostInstallUpdate, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let install_script = match install_script_path() {
            Some(path) => path,
            None => {
                let _ = tx.send(Err(
                    "No bundled installer is available in this build. Open the source checkout and run the host install script manually."
                        .into(),
                ));
                return;
            }
        };

        if uses_repo_assets(&install_script) {
            let repo_root = match repo_root() {
                Some(root) => root,
                None => {
                    let _ = tx.send(Err(
                        "Could not locate the SayWrite source repository. Run this build from the repo or use a Flatpak build that bundles the host installer."
                            .into(),
                    ));
                    return;
                }
            };

            let binary = repo_root.join("target/release/saywrite-host");
            if !binary.exists() {
                let _ = tx.send(Ok(HostInstallUpdate::Progress(
                    "Building saywrite-host — this may take a minute…".into(),
                )));
                match Command::new("cargo")
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
        }

        let _ = tx.send(Ok(HostInstallUpdate::Progress(
            "Installing host companion…".into(),
        )));

        let mut command = Command::new("bash");
        command.arg(&install_script);
        if let Some(dir) = install_script.parent() {
            command.current_dir(dir);
        }

        match command.output() {
            Ok(out) if out.status.success() => {
                let _ = tx.send(Ok(HostInstallUpdate::Done));
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                let text = if !stderr.is_empty() { stderr } else { stdout };
                let snippet = text.lines().take(8).collect::<Vec<_>>().join("
");
                let _ = tx.send(Err(format!("Install failed:\n{snippet}")));
            }
            Err(err) => {
                let _ = tx.send(Err(format!("Failed to run install script: {err}")));
            }
        }
    });
    rx
}

pub fn apply_shortcut_change(shortcut: &str) -> Result<(), String> {
    if let Some(script) = gnome_shortcut_script_path() {
        let mut command = Command::new("bash");
        command.arg(&script).arg(shortcut);
        if let Some(dir) = script.parent() {
            command.current_dir(dir);
        }

        match command.output() {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                let text = if !stderr.trim().is_empty() { stderr } else { stdout };
                let snippet = text.lines().take(6).collect::<Vec<_>>().join("\n");
                return Err(format!("Failed to apply desktop shortcut:\n{snippet}"));
            }
            Err(err) => return Err(format!("Failed to run shortcut helper: {err}")),
        }
    }

    restart_host_service();
    Ok(())
}

pub fn host_install_instructions() -> String {
    let setup = host_setup_status();
    let mut steps = vec![
        "Enable Direct Typing:".to_string(),
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
    steps.join("
")
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
    if bundled_asset_root().is_some() {
        return "Press Install in Settings.".into();
    }

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

    if bundled_asset_root().is_some() {
        return Some("Press Install, then use the bundled GNOME shortcut helper if needed.".into());
    }

    if let Some(repo_root) = repo_root() {
        if repo_root.join("scripts/install-gnome-shortcut.sh").exists() {
            return Some("bash scripts/install-gnome-shortcut.sh".into());
        }
    }

    Some("Create a GNOME custom shortcut that runs the SayWrite host toggle command.".into())
}

fn gnome_shortcut_script_path() -> Option<PathBuf> {
    if let Some(root) = bundled_asset_root() {
        let script = root.join("install-gnome-shortcut.sh");
        let toggle_helper = root.join("run-global-dictation.sh");
        if script.exists() && toggle_helper.exists() {
            return Some(script);
        }
    }

    repo_root().and_then(|root| {
        let script = root.join("scripts/install-gnome-shortcut.sh");
        script.exists().then_some(script)
    })
}

fn bundled_asset_root() -> Option<PathBuf> {
    let path = PathBuf::from("/app/share/saywrite");
    path.join("install-host.sh").exists().then_some(path)
}

fn install_script_path() -> Option<PathBuf> {
    if let Some(root) = bundled_asset_root() {
        return Some(root.join("install-host.sh"));
    }

    repo_root().and_then(|root| {
        let script = root.join("scripts/install-host.sh");
        script.exists().then_some(script)
    })
}

fn uses_repo_assets(install_script: &Path) -> bool {
    !install_script.starts_with("/app/")
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

fn restart_host_service() {
    let _ = if Path::new("/.flatpak-info").exists() {
        Command::new("flatpak-spawn")
            .args(["--host", "systemctl", "--user", "restart", "saywrite-host.service"])
            .status()
    } else {
        Command::new("systemctl")
            .args(["--user", "restart", "saywrite-host.service"])
            .status()
    };
}
