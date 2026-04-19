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

#[derive(Debug, Clone)]
pub struct HostDiagnostics {
    pub desktop_label: String,
    pub host_files_label: String,
    pub dependency_label: String,
    pub package_hint: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostProfile {
    GnomeWayland,
    OtherWayland,
    X11,
    Other,
}

#[derive(Clone, Copy, Debug)]
struct CommandRequirement {
    command: &'static str,
    package_hint: Option<&'static str>,
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
        binary_installed: host_path_exists(&binary_path),
        systemd_service_installed: host_path_exists(&systemd_service_path),
        dbus_service_installed: host_path_exists(&dbus_service_path),
        host_running: host_integration::host_status().is_some(),
        install_command: host_install_command(),
        gnome_shortcut_command: gnome_shortcut_command(),
    }
}

pub fn host_diagnostics() -> HostDiagnostics {
    let setup = host_setup_status();
    let profile = host_profile();
    let missing = missing_requirements(profile);

    HostDiagnostics {
        desktop_label: desktop_label(profile),
        host_files_label: host_files_label(&setup),
        dependency_label: dependency_label(profile, &missing),
        package_hint: dependency_package_hint(&missing),
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
                let snippet = text.lines().take(8).collect::<Vec<_>>().join("\n");
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
    if gnome_shortcuts_supported() {
        // Inside Flatpak the bundled bash script cannot update the host dconf
        // and would set an invalid command path (/app/…). Update the binding
        // value directly via gsettings on the host instead.
        if inside_flatpak() {
            if let Err(e) = apply_gnome_binding_via_gsettings(shortcut) {
                eprintln!("gsettings shortcut update failed (non-fatal): {e}");
            }
        } else if let Some(script) = gnome_shortcut_script_path() {
            let mut command = Command::new("bash");
            command.arg(&script).arg(shortcut);
            if let Some(dir) = script.parent() {
                command.current_dir(dir);
            }

            match command.output() {
                Ok(out) if !out.status.success() => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let text = if !stderr.trim().is_empty() {
                        stderr
                    } else {
                        stdout
                    };
                    let snippet = text.lines().take(6).collect::<Vec<_>>().join("\n");
                    return Err(format!("Failed to apply desktop shortcut:\n{snippet}"));
                }
                Err(err) => return Err(format!("Failed to run shortcut helper: {err}")),
                _ => {}
            }
        } else {
            // No script available outside Flatpak — try gsettings directly.
            if let Err(e) = apply_gnome_binding_via_gsettings(shortcut) {
                eprintln!("gsettings shortcut update failed (non-fatal): {e}");
            }
        }
    }

    restart_host_service();
    Ok(())
}

/// Convert "Super+Ctrl+Alt+Shift+A" → "<Super><Primary><Alt><Shift>a" for gsettings.
fn shortcut_to_gnome_binding(label: &str) -> String {
    let mut modifiers = String::new();
    let mut key = String::new();

    for part in label.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "super" => modifiers.push_str("<Super>"),
            "ctrl" | "control" => modifiers.push_str("<Primary>"),
            "alt" => modifiers.push_str("<Alt>"),
            "shift" => modifiers.push_str("<Shift>"),
            other if !other.is_empty() => key = other.to_string(),
            _ => {}
        }
    }

    if key.is_empty() {
        key = "d".to_string();
    }

    format!("{modifiers}{key}")
}

/// Temporarily disable the GNOME custom keybinding so the capture dialog
/// can receive all key combos.
pub fn suspend_gnome_shortcut() {
    if gnome_shortcuts_supported() {
        let _ = set_gnome_binding_raw("");
    }
}

/// Re-enable the GNOME custom keybinding with the given shortcut label.
pub fn restore_gnome_shortcut(shortcut_label: &str) {
    if gnome_shortcuts_supported() {
        let binding = shortcut_to_gnome_binding(shortcut_label);
        let _ = set_gnome_binding_raw(&binding);
    }
}

fn apply_gnome_binding_via_gsettings(shortcut: &str) -> Result<(), String> {
    let binding = shortcut_to_gnome_binding(shortcut);
    set_gnome_binding_raw(&binding)
}

fn set_gnome_binding_raw(binding: &str) -> Result<(), String> {
    const SCHEMA_KEY: &str = concat!(
        "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding",
        ":/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite-hands-free/"
    );

    let args: Vec<&str> = vec!["gsettings", "set", SCHEMA_KEY, "binding", binding];

    let out = if inside_flatpak() {
        let mut flatpak_args = vec!["--host"];
        flatpak_args.extend_from_slice(&args);
        Command::new("flatpak-spawn")
            .args(&flatpak_args)
            .output()
            .map_err(|e| format!("flatpak-spawn: {e}"))?
    } else {
        Command::new(args[0])
            .args(&args[1..])
            .output()
            .map_err(|e| format!("gsettings: {e}"))?
    };

    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).into_owned();
        Err(msg)
    }
}

fn inside_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn host_profile() -> HostProfile {
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let session = env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let gnome = desktop
        .split(':')
        .any(|part| part.eq_ignore_ascii_case("gnome"));

    if session.eq_ignore_ascii_case("wayland") && gnome {
        HostProfile::GnomeWayland
    } else if session.eq_ignore_ascii_case("wayland") {
        HostProfile::OtherWayland
    } else if session.eq_ignore_ascii_case("x11") {
        HostProfile::X11
    } else {
        HostProfile::Other
    }
}

fn host_path_exists(path: &Path) -> bool {
    if !inside_flatpak() {
        return path.exists();
    }

    let Some(path_str) = path.to_str() else {
        eprintln!("host path check skipped for non-UTF-8 path: {:?}", path);
        return false;
    };

    match Command::new("flatpak-spawn")
        .args(["--host", "test", "-e", path_str])
        .status()
    {
        Ok(status) => status.success(),
        Err(err) => {
            eprintln!("host path check failed for {}: {}", path.display(), err);
            false
        }
    }
}

fn host_command_exists(command: &str) -> bool {
    let probe = format!("command -v {command} >/dev/null 2>&1");
    let status = if inside_flatpak() {
        Command::new("flatpak-spawn")
            .args(["--host", "sh", "-lc", &probe])
            .status()
    } else {
        Command::new("sh").args(["-lc", &probe]).status()
    };

    match status {
        Ok(status) => status.success(),
        Err(err) => {
            eprintln!("host command probe failed for {}: {}", command, err);
            false
        }
    }
}

fn desktop_label(profile: HostProfile) -> String {
    match profile {
        HostProfile::GnomeWayland => "GNOME Wayland".into(),
        HostProfile::OtherWayland => "Wayland".into(),
        HostProfile::X11 => "X11".into(),
        HostProfile::Other => {
            let desktop =
                env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "Unknown desktop".into());
            let session = env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown session".into());
            format!("{desktop} ({session})")
        }
    }
}

fn host_files_label(setup: &HostSetupStatus) -> String {
    let mut missing = Vec::new();
    if !setup.binary_installed {
        missing.push("binary");
    }
    if !setup.systemd_service_installed {
        missing.push("systemd user service");
    }
    if !setup.dbus_service_installed {
        missing.push("D-Bus service");
    }

    if missing.is_empty() {
        "Host binary, systemd unit, and D-Bus service are present.".into()
    } else {
        format!("Missing host files: {}.", missing.join(", "))
    }
}

fn requirements_for_profile(profile: HostProfile) -> &'static [CommandRequirement] {
    const GNOME_WAYLAND: &[CommandRequirement] = &[
        CommandRequirement {
            command: "ibus",
            package_hint: Some("ibus"),
        },
        CommandRequirement {
            command: "gdbus",
            package_hint: Some("libglib2.0-bin"),
        },
        CommandRequirement {
            command: "busctl",
            package_hint: Some("systemd"),
        },
    ];
    const OTHER_WAYLAND: &[CommandRequirement] = &[
        CommandRequirement {
            command: "wtype",
            package_hint: Some("wtype"),
        },
        CommandRequirement {
            command: "busctl",
            package_hint: Some("systemd"),
        },
    ];
    const X11_REQS: &[CommandRequirement] = &[
        CommandRequirement {
            command: "xdotool",
            package_hint: Some("xdotool"),
        },
        CommandRequirement {
            command: "busctl",
            package_hint: Some("systemd"),
        },
    ];
    const OTHER: &[CommandRequirement] = &[];

    match profile {
        HostProfile::GnomeWayland => GNOME_WAYLAND,
        HostProfile::OtherWayland => OTHER_WAYLAND,
        HostProfile::X11 => X11_REQS,
        HostProfile::Other => OTHER,
    }
}

fn missing_requirements(profile: HostProfile) -> Vec<CommandRequirement> {
    requirements_for_profile(profile)
        .iter()
        .copied()
        .filter(|req| !host_command_exists(req.command))
        .collect()
}

fn dependency_label(profile: HostProfile, missing: &[CommandRequirement]) -> String {
    if missing.is_empty() {
        return match profile {
            HostProfile::GnomeWayland => {
                "GNOME Wayland host checks look ready for Direct Typing.".into()
            }
            HostProfile::OtherWayland => {
                "Wayland host checks look ready for the current fallback path.".into()
            }
            HostProfile::X11 => "X11 host checks look ready for Direct Typing.".into(),
            HostProfile::Other => {
                "No desktop-specific host checks are defined for this session.".into()
            }
        };
    }

    let names = missing
        .iter()
        .map(|req| req.command)
        .collect::<Vec<_>>()
        .join(", ");
    match profile {
        HostProfile::GnomeWayland => format!("Missing GNOME Wayland host tools: {names}."),
        HostProfile::OtherWayland => format!("Missing Wayland host tools: {names}."),
        HostProfile::X11 => format!("Missing X11 host tools: {names}."),
        HostProfile::Other => format!("Missing host tools: {names}."),
    }
}

fn dependency_package_hint(missing: &[CommandRequirement]) -> Option<String> {
    let distro = host_os_release();
    let ubuntu_like = distro
        .get("ID")
        .into_iter()
        .chain(distro.get("ID_LIKE"))
        .any(|value| {
            value
                .split_whitespace()
                .any(|part| matches!(part, "ubuntu" | "debian" | "zorin"))
        });

    if !ubuntu_like {
        return None;
    }

    let mut packages = Vec::new();
    for req in missing {
        if let Some(pkg) = req.package_hint {
            if !packages.contains(&pkg) {
                packages.push(pkg);
            }
        }
    }

    if packages.is_empty() {
        None
    } else {
        Some(format!(
            "Ubuntu/Zorin host packages: sudo apt install {}",
            packages.join(" ")
        ))
    }
}

fn host_os_release() -> std::collections::HashMap<String, String> {
    let text = if inside_flatpak() {
        Command::new("flatpak-spawn")
            .args(["--host", "cat", "/etc/os-release"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        std::fs::read_to_string("/etc/os-release").ok()
    };

    text.map(parse_os_release).unwrap_or_default()
}

fn parse_os_release(text: String) -> std::collections::HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.to_string(), value.trim_matches('"').to_string()))
        })
        .collect()
}

/// Fetch the last few lines of the host daemon's journal, used to explain
/// why the service appears installed but unreachable. Runs on the host.
pub fn host_daemon_journal_tail(lines: u32) -> Option<String> {
    let lines_str = lines.to_string();
    let args = [
        "journalctl",
        "--user",
        "-u",
        "saywrite-host.service",
        "--no-pager",
        "-n",
        &lines_str,
    ];

    let output = if inside_flatpak() {
        let mut spawn_args = vec!["--host"];
        spawn_args.extend_from_slice(&args);
        Command::new("flatpak-spawn").args(&spawn_args).output()
    } else {
        Command::new(args[0]).args(&args[1..]).output()
    }
    .ok()?;

    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn host_install_instructions() -> String {
    let setup = host_setup_status();
    let diagnostics = host_diagnostics();
    let mut steps = vec![
        "Enable Direct Typing:".to_string(),
        String::new(),
        format!("1. {}", setup.install_command),
    ];

    if let Some(command) = setup.gnome_shortcut_command.as_ref() {
        steps.push(format!("2. Optional GNOME fallback shortcut: {command}"));
    }

    steps.push(String::new());
    steps.push(format!("Host session: {}", diagnostics.desktop_label));
    steps.push(format!("Host checks: {}", diagnostics.dependency_label));
    if let Some(hint) = diagnostics.package_hint {
        steps.push(hint);
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
            .args([
                "--host",
                "systemctl",
                "--user",
                "restart",
                "saywrite-host.service",
            ])
            .status()
    } else {
        Command::new("systemctl")
            .args(["--user", "restart", "saywrite-host.service"])
            .status()
    };
}
