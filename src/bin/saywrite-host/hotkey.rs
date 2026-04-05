use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Context, Result};
use saywrite::config::AppSettings;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::{Connection, MessageStream, Proxy};

use tokio_stream::StreamExt;

static PORTAL_ACTIVE: AtomicBool = AtomicBool::new(false);
const TOGGLE_SHORTCUT_ID: &str = "toggle-dictation";

#[derive(Debug, Clone)]
pub struct HotkeyStatus {
    pub active: bool,
    pub message: String,
    pub setup_hint: String,
}

pub fn probe(settings: &AppSettings) -> HotkeyStatus {
    let shortcut = settings.global_shortcut_label.clone();

    if PORTAL_ACTIVE.load(Ordering::Relaxed) {
        return HotkeyStatus {
            active: true,
            message: format!("GlobalShortcuts portal active for {}.", shortcut),
            setup_hint: String::new(),
        };
    }

    if portal_interface_available() {
        return HotkeyStatus {
            active: false,
            message: format!(
                "GlobalShortcuts portal detected. Registration pending for {}.",
                shortcut
            ),
            setup_hint: custom_shortcut_hint(&shortcut),
        };
    }

    if gnome_shortcuts_supported() {
        return HotkeyStatus {
            active: false,
            message: format!(
                "GlobalShortcuts portal is unavailable here. Use the GNOME shortcut fallback for {}.",
                shortcut
            ),
            setup_hint: gnome_shortcut_hint(),
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

/// Register a global shortcut via the XDG GlobalShortcuts portal and listen
/// for activations. On each activation, calls ToggleDictation on the host
/// D-Bus interface.
pub async fn register_and_listen() -> Result<()> {
    let conn = Connection::session().await.context("no D-Bus session bus")?;

    let portal = Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.GlobalShortcuts",
    )
    .await
    .context("failed to create GlobalShortcuts proxy")?;

    // 1. CreateSession
    let session_handle = create_session(&conn, &portal).await?;

    // 2. BindShortcuts
    let settings = AppSettings::load();
    bind_shortcuts(&conn, &portal, &session_handle, &settings.global_shortcut_label).await?;

    PORTAL_ACTIVE.store(true, Ordering::Relaxed);
    eprintln!("GlobalShortcuts portal: shortcut bound, listening for activations");

    // 3. Listen for Activated signal
    let mut stream = MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => continue,
        };
        let header = match msg.header() {
            Ok(h) => h,
            Err(_) => continue,
        };
        let member = header
            .member()
            .ok()
            .flatten()
            .map(|m| m.as_str().to_string());
        let interface = header
            .interface()
            .ok()
            .flatten()
            .map(|i| i.as_str().to_string());

        if interface.as_deref() == Some("org.freedesktop.portal.GlobalShortcuts")
            && member.as_deref() == Some("Activated")
        {
            let body = msg.body::<(ObjectPath<'_>, String, u64, HashMap<String, OwnedValue>)>();
            let (activated_session, shortcut_id, _timestamp, _options) = match body {
                Ok(values) => values,
                Err(err) => {
                    eprintln!("GlobalShortcuts: failed to parse Activated body: {err}");
                    continue;
                }
            };

            if activated_session.as_str() != session_handle.as_str() {
                continue;
            }
            if shortcut_id != TOGGLE_SHORTCUT_ID {
                continue;
            }

            eprintln!("GlobalShortcuts: activation received, toggling dictation");
            toggle_dictation_via_dbus(&conn).await;
        }
    }

    PORTAL_ACTIVE.store(false, Ordering::Relaxed);
    Ok(())
}

async fn create_session(conn: &Connection, portal: &Proxy<'_>) -> Result<ObjectPath<'static>> {
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    let token = format!("saywrite_{}", std::process::id());
    let session_token = format!("saywrite_session_{}", std::process::id());
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("session_handle_token", Value::from(session_token.as_str()));

    let unique_name = conn
        .unique_name()
        .ok_or_else(|| anyhow!("no unique bus name"))?
        .as_str()
        .replace('.', "_")
        .trim_start_matches(':')
        .to_string();
    let request_path = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        unique_name, token
    );

    let mut response_stream = MessageStream::from(conn.clone());

    let _reply: ObjectPath = portal
        .call_method("CreateSession", &(options,))
        .await
        .context("CreateSession call failed")?
        .body()
        .context("CreateSession reply parse failed")?;

    // Wait for the Response signal
    let session_handle = wait_for_response(&mut response_stream, &request_path).await?;
    Ok(session_handle)
}

async fn bind_shortcuts(
    conn: &Connection,
    portal: &Proxy<'_>,
    session_handle: &ObjectPath<'_>,
    shortcut_label: &str,
) -> Result<()> {
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    let token = format!("saywrite_bind_{}", std::process::id());
    options.insert("handle_token", Value::from(token.as_str()));

    let mut shortcut_props: HashMap<&str, Value<'_>> = HashMap::new();
    shortcut_props.insert("description", Value::from("Toggle dictation"));
    shortcut_props.insert("preferred_trigger", Value::from(shortcut_label));

    let shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)> =
        vec![(TOGGLE_SHORTCUT_ID, shortcut_props)];

    let unique_name = conn
        .unique_name()
        .ok_or_else(|| anyhow!("no unique bus name"))?
        .as_str()
        .replace('.', "_")
        .trim_start_matches(':')
        .to_string();
    let request_path = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        unique_name, token
    );
    let mut response_stream = MessageStream::from(conn.clone());

    let _reply: ObjectPath = portal
        .call_method("BindShortcuts", &(session_handle, shortcuts, "", options))
        .await
        .context("BindShortcuts call failed")?
        .body()
        .context("BindShortcuts reply parse failed")?;

    wait_for_response(&mut response_stream, &request_path).await?;
    Ok(())
}

async fn wait_for_response(
    stream: &mut MessageStream,
    request_path: &str,
) -> Result<ObjectPath<'static>> {
    use tokio::time::{timeout, Duration};

    let result = timeout(Duration::from_secs(30), async {
        while let Some(msg) = stream.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(_) => continue,
            };
            let header = match msg.header() {
                Ok(h) => h,
                Err(_) => continue,
            };
            let path = header.path().ok().flatten().map(|p| p.as_str().to_string());
            let member = header.member().ok().flatten().map(|m| m.as_str().to_string());
            let interface = header.interface().ok().flatten().map(|i| i.as_str().to_string());

            if path.as_deref() == Some(request_path)
                && interface.as_deref() == Some("org.freedesktop.portal.Request")
                && member.as_deref() == Some("Response")
            {
                let body: (u32, HashMap<String, OwnedValue>) = msg
                    .body()
                    .context("failed to parse Response body")?;
                let (response_code, results) = body;
                if response_code != 0 {
                    return Err(anyhow!(
                        "Portal request denied (response code {})",
                        response_code
                    ));
                }
                if let Some(session_val) = results.get("session_handle") {
                    let session_str: String = session_val
                        .clone()
                        .try_into()
                        .unwrap_or_default();
                    if !session_str.is_empty() {
                        return Ok(ObjectPath::try_from(session_str)
                            .context("invalid session handle path")?
                            .into_owned());
                    }
                }
                // No session handle in response (e.g. BindShortcuts)
                return Ok(ObjectPath::try_from("/org/freedesktop/portal/desktop")
                    .unwrap()
                    .into_owned());
            }
        }
        Err(anyhow!("D-Bus stream ended while waiting for portal response"))
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(anyhow!("Timed out waiting for portal response")),
    }
}

async fn toggle_dictation_via_dbus(conn: &Connection) {
    let proxy = match Proxy::new(
        conn,
        saywrite::host_api::BUS_NAME,
        saywrite::host_api::OBJECT_PATH,
        saywrite::host_api::INTERFACE_NAME,
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to create host proxy for toggle: {e}");
            return;
        }
    };

    match proxy.call_method("ToggleDictation", &()).await {
        Ok(_) => {}
        Err(e) => eprintln!("ToggleDictation call failed: {e}"),
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

fn portal_interface_available() -> bool {
    if !portal_available() {
        return false;
    }

    let output = Command::new("busctl")
        .args([
            "--user",
            "introspect",
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
        ])
        .output();

    match output {
        Ok(result) if result.status.success() => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            stdout.contains("org.freedesktop.portal.GlobalShortcuts")
        }
        _ => false,
    }
}

fn gnome_shortcuts_supported() -> bool {
    let current = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    current.split(':').any(|part| part.eq_ignore_ascii_case("gnome"))
}

fn custom_shortcut_hint(shortcut: &str) -> String {
    format!(
        "Create a desktop shortcut for `{}` that runs: busctl --user call io.github.saywrite.Host /io/github/saywrite/Host io.github.saywrite.Host ToggleDictation",
        shortcut
    )
}

fn gnome_shortcut_hint() -> String {
    "Install the GNOME fallback shortcut with: bash scripts/install-gnome-shortcut.sh".into()
}
