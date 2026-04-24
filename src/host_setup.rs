use std::{env, fs, path::PathBuf, process::Command};

use crate::host_integration;

#[derive(Debug, Clone)]
pub struct HostSetupStatus {
    pub host_running: bool,
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

pub fn host_setup_status() -> HostSetupStatus {
    HostSetupStatus {
        host_running: host_integration::host_status().is_some(),
    }
}

pub fn host_diagnostics() -> HostDiagnostics {
    let profile = host_profile();
    let missing = missing_requirements(profile);

    HostDiagnostics {
        desktop_label: desktop_label(profile),
        host_files_label: "Direct typing is built into the native app.".into(),
        dependency_label: dependency_label(profile, &missing),
        package_hint: dependency_package_hint(&missing),
    }
}

pub fn cleanup_legacy_host_companion() {
    let mut changed = false;

    for args in [
        &["systemctl", "--user", "stop", "saywrite-host.service"][..],
        &["systemctl", "--user", "disable", "saywrite-host.service"][..],
    ] {
        let _ = run_local(args);
    }

    for path in legacy_host_paths() {
        if path.exists() {
            match fs::remove_file(&path) {
                Ok(()) => {
                    changed = true;
                    eprintln!(
                        "SayWrite: removed legacy host companion file {}",
                        path.display()
                    );
                }
                Err(err) => {
                    eprintln!(
                        "SayWrite: could not remove legacy host companion file {}: {err}",
                        path.display()
                    );
                }
            }
        }
    }

    if changed {
        let _ = run_local(&["systemctl", "--user", "daemon-reload"]);
    }
}

pub fn apply_shortcut_change(shortcut: &str) -> Result<(), String> {
    if gnome_shortcuts_supported() {
        ensure_gnome_shortcut(shortcut)?;
    }
    host_integration::restart_shortcut_listener();
    Ok(())
}

pub fn suspend_gnome_shortcut() {
    if gnome_shortcuts_supported() {
        let _ = set_gnome_binding_raw("");
    }
}

pub fn restore_gnome_shortcut(shortcut_label: &str) {
    if gnome_shortcuts_supported() {
        let binding = shortcut_to_gnome_binding(shortcut_label);
        let _ = set_gnome_binding_raw(&binding);
    }
}

pub fn ensure_gnome_shortcut(label: &str) -> Result<(), String> {
    if !gnome_shortcuts_supported() {
        return Ok(());
    }

    let command = hands_free_command_path().ok_or_else(|| {
        "dictation launcher not found; install the native package or run from the repo checkout"
            .to_string()
    })?;

    const RECONCILE_PY: &str = concat!(
        "import ast, subprocess, sys\n",
        "raw = subprocess.check_output(['gsettings','get','org.gnome.settings-daemon.plugins.media-keys','custom-keybindings']).decode()\n",
        "try:\n",
        "    paths = ast.literal_eval(raw.strip())\n",
        "except Exception:\n",
        "    paths = []\n",
        "keep_path = sys.argv[1]\n",
        "drop = set(sys.argv[2:])\n",
        "result = [p for p in paths if p not in drop and p != keep_path]\n",
        "result.append(keep_path)\n",
        "subprocess.check_call(['gsettings','set','org.gnome.settings-daemon.plugins.media-keys','custom-keybindings', repr(result)])\n",
    );

    let mut reconcile_args: Vec<&str> = vec!["python3", "-c", RECONCILE_PY, HANDS_FREE_PATH];
    reconcile_args.extend_from_slice(LEGACY_PATHS);
    run_local(&reconcile_args)?;

    set_gnome_keybinding_field("name", "SayWrite Hands-Free Dictation")?;
    set_gnome_keybinding_field("command", &command)?;
    let binding = shortcut_to_gnome_binding(label);
    set_gnome_keybinding_field("binding", &binding)?;

    let actual = get_gnome_keybinding_field("binding").unwrap_or_default();
    if actual != binding {
        return Err(format!(
            "GNOME accepted the keybinding write but binding is {:?}, expected {:?}",
            actual, binding
        ));
    }

    Ok(())
}

pub fn self_heal_gnome_shortcut(label: &str) {
    if !gnome_shortcuts_supported() {
        return;
    }

    let expected_binding = shortcut_to_gnome_binding(label);
    let current_binding = get_gnome_keybinding_field("binding").unwrap_or_default();
    let current_command = get_gnome_keybinding_field("command").unwrap_or_default();
    let expected_command = hands_free_command_path();
    let needs_fix = current_binding.is_empty()
        || current_binding != expected_binding
        || current_command.is_empty()
        || expected_command
            .as_deref()
            .map(|command| command != current_command)
            .unwrap_or(false);

    if !needs_fix {
        return;
    }

    if let Err(err) = ensure_gnome_shortcut(label) {
        eprintln!("SayWrite: could not install GNOME shortcut automatically: {err}");
    } else {
        eprintln!("SayWrite: installed GNOME hands-free shortcut for {label}");
    }
}

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
        key = "d".into();
    }

    format!("{modifiers}{key}")
}

fn set_gnome_binding_raw(binding: &str) -> Result<(), String> {
    set_gnome_keybinding_field("binding", binding)
}

const HANDS_FREE_SCHEMA_KEY: &str = concat!(
    "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding",
    ":/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite-hands-free/"
);
const HANDS_FREE_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite-hands-free/";
const LEGACY_PATHS: &[&str] = &[
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite/",
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite-quick/",
];

fn set_gnome_keybinding_field(field: &str, value: &str) -> Result<(), String> {
    let args = ["gsettings", "set", HANDS_FREE_SCHEMA_KEY, field, value];
    run_local(&args)
}

fn get_gnome_keybinding_field(field: &str) -> Result<String, String> {
    let args = ["gsettings", "get", HANDS_FREE_SCHEMA_KEY, field];
    let out = capture_local(&args)?;
    Ok(String::from_utf8_lossy(&out)
        .trim()
        .trim_matches('\'')
        .to_string())
}

fn run_local(args: &[&str]) -> Result<(), String> {
    let out = Command::new(args[0])
        .args(&args[1..])
        .output()
        .map_err(|e| format!("{}: {e}", args[0]))?;

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn capture_local(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new(args[0])
        .args(&args[1..])
        .output()
        .map_err(|e| format!("{}: {e}", args[0]))?;

    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn hands_free_command_path() -> Option<String> {
    let packaged = PathBuf::from("/usr/bin/saywrite-dictation.sh");
    if packaged.is_file() {
        return Some(packaged.to_string_lossy().into_owned());
    }

    let repo = find_repo_root_for_dev()?;
    let script = repo.join("scripts/run-global-dictation.sh");
    if script.exists() {
        Some(script.to_string_lossy().into_owned())
    } else {
        None
    }
}

fn legacy_host_paths() -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|path| path.join(".config")));
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|path| path.join(".local/share")));

    let mut paths = Vec::new();
    if let Some(config_home) = config_home {
        paths.push(config_home.join("systemd/user/saywrite-host.service"));
    }
    if let Some(data_home) = data_home {
        paths.push(data_home.join("dbus-1/services/io.github.saywrite.Host.service"));
    }
    if let Some(home) = home {
        paths.push(home.join(".local/bin/saywrite-host"));
    }
    paths
}

fn find_repo_root_for_dev() -> Option<PathBuf> {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let cwd = env::current_dir().ok();

    [exe_dir, cwd]
        .into_iter()
        .flatten()
        .find_map(find_repo_root)
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

fn host_command_exists(command: &str) -> bool {
    let probe = format!("command -v {command} >/dev/null 2>&1");
    match Command::new("sh").args(["-lc", &probe]).status() {
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
                "GNOME Wayland checks look ready for Direct Typing.".into()
            }
            HostProfile::OtherWayland => {
                "Wayland checks look ready for the current fallback path.".into()
            }
            HostProfile::X11 => "X11 checks look ready for Direct Typing.".into(),
            HostProfile::Other => "No desktop-specific checks are defined for this session.".into(),
        };
    }

    let names = missing
        .iter()
        .map(|req| req.command)
        .collect::<Vec<_>>()
        .join(", ");

    match profile {
        HostProfile::GnomeWayland => format!("Missing GNOME Wayland tools: {names}."),
        HostProfile::OtherWayland => format!("Missing Wayland tools: {names}."),
        HostProfile::X11 => format!("Missing X11 tools: {names}."),
        HostProfile::Other => format!("Missing desktop tools: {names}."),
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
            "Ubuntu/Zorin packages: sudo apt install {}",
            packages.join(" ")
        ))
    }
}

fn host_os_release() -> std::collections::HashMap<String, String> {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .map(parse_os_release)
        .unwrap_or_default()
}

fn parse_os_release(text: String) -> std::collections::HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.to_string(), value.trim_matches('"').to_string()))
        })
        .collect()
}

fn find_repo_root(start: PathBuf) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        if candidate.join("Cargo.toml").exists()
            && candidate.join("scripts/run-global-dictation.sh").exists()
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
