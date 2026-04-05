use std::process::Command;

use saywrite::config::AppSettings;

#[derive(Debug, Clone)]
pub struct HotkeyStatus {
    pub active: bool,
    pub message: String,
    pub setup_hint: String,
}

pub fn probe(settings: &AppSettings) -> HotkeyStatus {
    let shortcut = settings.global_shortcut_label.clone();

    if portal_available() {
        return HotkeyStatus {
            active: false,
            message: format!(
                "GlobalShortcuts portal detected. Native registration is still pending for {}.",
                shortcut
            ),
            setup_hint: custom_shortcut_hint(&shortcut),
        };
    }

    HotkeyStatus {
        active: false,
        message: format!(
            "No supported global shortcut backend detected. {} is not armed automatically.",
            shortcut
        ),
        setup_hint: custom_shortcut_hint(&shortcut),
    }
}

fn portal_available() -> bool {
    let output = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            "s",
            "org.freedesktop.portal.Desktop",
        ])
        .output();

    match output {
        Ok(result) if result.status.success() => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            stdout.contains("true")
        }
        _ => false,
    }
}

fn custom_shortcut_hint(shortcut: &str) -> String {
    format!(
        "Create a desktop shortcut for `{}` that runs: busctl --user call io.github.saywrite.Host /io/github/saywrite/Host io.github.saywrite.Host ToggleDictation",
        shortcut
    )
}
