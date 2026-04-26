use std::{
    collections::HashMap,
    env,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    thread,
    time::Duration,
};

use crate::config::AppSettings;
use anyhow::{anyhow, Context, Result};
use tokio::{
    sync::{oneshot, Mutex},
    time::timeout,
};
use tokio_stream::StreamExt;
use zbus::{
    dbus_interface, fdo,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Structure, StructureBuilder, Value},
    Connection, ConnectionBuilder, MessageStream, Proxy, SignalContext,
};

// ---------------------------------------------------------------------------
// Private environment helpers
// ---------------------------------------------------------------------------

fn gnome_desktop() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .any(|p| p.eq_ignore_ascii_case("gnome"))
}

fn wayland_session() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
}

// ===========================================================================
// IBus bridge
// ===========================================================================

const COMPONENT_NAME: &str = "io.github.saywrite.IBus";
const ENGINE_NAME: &str = "io.github.saywrite.Engine";
const FACTORY_PATH: &str = "/org/freedesktop/IBus/Factory";
const ENGINE_PATH: &str = "/org/freedesktop/IBus/engine/saywrite/1";
const IBUS_DEST: &str = "org.freedesktop.IBus";
const IBUS_PATH: &str = "/org/freedesktop/IBus";
const IBUS_IFACE: &str = "org.freedesktop.IBus";

static BRIDGE: OnceLock<IbusBridge> = OnceLock::new();

#[derive(Clone)]
pub struct IbusBridge {
    inner: Arc<IbusBridgeInner>,
}

struct IbusBridgeInner {
    connection: Mutex<Option<Connection>>,
    pending_commit: Mutex<Option<PendingCommit>>,
}

struct PendingCommit {
    text: String,
    previous_engine: String,
    response: oneshot::Sender<Result<String, String>>,
}

struct IbusService;

struct IbusFactory;

struct IbusEngine {
    inner: Arc<IbusBridgeInner>,
}

pub fn preferred_on_this_desktop() -> bool {
    wayland_session()
}

pub fn gnome_wayland() -> bool {
    gnome_desktop() && wayland_session()
}

pub fn bridge_ready() -> bool {
    BRIDGE.get().is_some()
}

pub async fn ensure_bridge() -> Result<()> {
    if BRIDGE.get().is_some() {
        return Ok(());
    }

    ensure_running()?;

    let inner = Arc::new(IbusBridgeInner {
        connection: Mutex::new(None),
        pending_commit: Mutex::new(None),
    });

    let connection = ConnectionBuilder::address(address()?.as_str())?
        .name(COMPONENT_NAME)?
        .serve_at(FACTORY_PATH, IbusFactory)?
        .serve_at(FACTORY_PATH, IbusService)?
        .serve_at(
            ENGINE_PATH,
            IbusEngine {
                inner: inner.clone(),
            },
        )?
        .serve_at(ENGINE_PATH, IbusService)?
        .build()
        .await
        .context("failed to connect SayWrite to the IBus private bus")?;

    {
        let mut guard = inner.connection.lock().await;
        *guard = Some(connection.clone());
    }

    register_component(&connection).await?;

    let _ = BRIDGE.set(IbusBridge { inner });
    Ok(())
}

pub async fn commit_text(text: &str) -> Result<String> {
    let bridge = BRIDGE
        .get()
        .cloned()
        .ok_or_else(|| anyhow!("SayWrite IBus bridge is not running"))?;
    bridge.commit_text(text).await
}

pub fn ensure_running() -> Result<()> {
    if address().is_ok() {
        return Ok(());
    }

    Command::new("ibus-daemon")
        .args(["-drx"])
        .status()
        .context("failed to start ibus-daemon")?;

    for _ in 0..10 {
        if address().is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(150));
    }

    Err(anyhow!(
        "ibus-daemon did not expose an address after startup"
    ))
}

pub fn address() -> Result<String> {
    let output = Command::new("ibus")
        .arg("address")
        .output()
        .context("failed to query ibus address")?;
    if !output.status.success() {
        return Err(anyhow!("ibus address exited with status {}", output.status));
    }

    let address = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if address.is_empty() || address == "(null)" {
        return Err(anyhow!("ibus daemon is not running"));
    }

    Ok(address)
}

pub fn current_input_context() -> Result<String> {
    let address = address()?;
    let output = Command::new("gdbus")
        .args([
            "call",
            "--address",
            &address,
            "--dest",
            IBUS_DEST,
            "--object-path",
            IBUS_PATH,
            "--method",
            "org.freedesktop.IBus.CurrentInputContext",
        ])
        .output()
        .context("failed to query current IBus input context")?;
    if !output.status.success() {
        return Err(anyhow!(
            "querying current IBus input context failed with status {}",
            output.status
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    parse_object_path(&text).ok_or_else(|| anyhow!("IBus did not return a current input context"))
}

pub fn global_engine_name() -> Result<String> {
    let address = address()?;
    let output = Command::new("gdbus")
        .args([
            "call",
            "--address",
            &address,
            "--dest",
            IBUS_DEST,
            "--object-path",
            IBUS_PATH,
            "--method",
            "org.freedesktop.IBus.GetGlobalEngine",
        ])
        .output()
        .context("failed to query global IBus engine")?;
    if !output.status.success() {
        return Err(anyhow!(
            "querying global IBus engine failed with status {}",
            output.status
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    parse_engine_name(&text).ok_or_else(|| anyhow!("IBus did not return a global engine name"))
}

impl IbusBridge {
    async fn commit_text(&self, text: &str) -> Result<String> {
        if text.trim().is_empty() {
            return Err(anyhow!("No text was provided for IBus insertion."));
        }

        current_input_context().context("no focused IBus input context is available")?;
        let previous_engine = global_engine_name().unwrap_or_else(|_| "xkb:us::eng".into());
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.inner.pending_commit.lock().await;
            if pending.is_some() {
                return Err(anyhow!(
                    "SayWrite is already committing text through the IBus bridge"
                ));
            }
            *pending = Some(PendingCommit {
                text: text.to_string(),
                previous_engine: previous_engine.clone(),
                response: tx,
            });
        }

        let connection = self
            .inner
            .connection
            .lock()
            .await
            .clone()
            .ok_or_else(|| anyhow!("SayWrite IBus bridge connection is not ready"))?;
        let proxy = Proxy::new(&connection, IBUS_DEST, IBUS_PATH, IBUS_IFACE)
            .await
            .context("failed to create an IBus proxy for SayWrite")?;

        eprintln!(
            "IBus: swapping engine from '{}' to SayWrite for text commit",
            previous_engine
        );
        if let Err(err) = proxy
            .call_method("SetGlobalEngine", &(ENGINE_NAME,))
            .await
            .context("failed to activate the SayWrite IBus engine")
        {
            self.clear_pending_commit().await;
            eprintln!("IBus: failed to set global engine to SayWrite: {err:#}");
            return Err(err);
        }

        match timeout(Duration::from_secs(3), rx).await {
            Ok(Ok(Ok(message))) => Ok(message),
            Ok(Ok(Err(message))) => {
                self.restore_after_failed_commit(&previous_engine).await;
                Err(anyhow!(message))
            }
            Ok(Err(_)) => {
                eprintln!("IBus: pending commit channel was dropped unexpectedly");
                self.restore_after_failed_commit(&previous_engine).await;
                Err(anyhow!("SayWrite IBus engine dropped the pending commit"))
            }
            Err(_) => {
                eprintln!("IBus: timed out waiting for SayWrite engine focus_in callback");
                self.restore_after_failed_commit(&previous_engine).await;
                Err(anyhow!(
                    "Timed out waiting for the SayWrite IBus engine to commit text"
                ))
            }
        }
    }

    async fn clear_pending_commit(&self) {
        let mut pending = self.inner.pending_commit.lock().await;
        pending.take();
    }

    async fn restore_after_failed_commit(&self, previous_engine: &str) {
        {
            let mut pending = self.inner.pending_commit.lock().await;
            pending.take();
        }

        let connection = self.inner.connection.lock().await.clone();
        if let Some(connection) = connection {
            if let Err(err) = restore_previous_engine(&connection, previous_engine).await {
                eprintln!(
                    "IBus: failed to restore engine '{}' after failed commit: {err:#}",
                    previous_engine
                );
            }
        }
    }
}

#[dbus_interface(name = "org.freedesktop.IBus.Service")]
impl IbusService {
    fn destroy(&self) {}
}

#[dbus_interface(name = "org.freedesktop.IBus.Factory")]
impl IbusFactory {
    async fn create_engine(&self, name: &str) -> fdo::Result<OwnedObjectPath> {
        if name != ENGINE_NAME {
            return Err(fdo::Error::Failed(format!(
                "SayWrite cannot create unknown engine {name}"
            )));
        }

        OwnedObjectPath::try_from(ENGINE_PATH)
            .map_err(|err| fdo::Error::Failed(format!("invalid SayWrite engine path: {err}")))
    }
}

#[dbus_interface(name = "org.freedesktop.IBus.Engine")]
impl IbusEngine {
    async fn process_key_event(&self, _keyval: u32, _keycode: u32, _state: u32) -> bool {
        false
    }

    async fn set_cursor_location(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}

    async fn process_hand_writing_event(&self, _coordinates: Vec<f64>) {}

    async fn cancel_hand_writing(&self, _n_strokes: u32) {}

    async fn set_capabilities(&self, _caps: u32) {}

    async fn property_activate(&self, _name: &str, _state: u32) {}

    async fn property_show(&self, _name: &str) {}

    async fn property_hide(&self, _name: &str) {}

    async fn candidate_clicked(&self, _index: u32, _button: u32, _state: u32) {}

    async fn focus_in(&self, #[zbus(signal_context)] ctxt: SignalContext<'_>) -> fdo::Result<()> {
        self.flush_pending_commit(&ctxt).await
    }

    async fn focus_in_id(
        &self,
        _object_path: &str,
        _client: &str,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<()> {
        self.flush_pending_commit(&ctxt).await
    }

    async fn focus_out(&self) {}

    async fn focus_out_id(&self, _object_path: &str) {}

    async fn reset(&self) {}

    async fn enable(&self, #[zbus(signal_context)] ctxt: SignalContext<'_>) -> fdo::Result<()> {
        self.flush_pending_commit(&ctxt).await
    }

    async fn disable(&self) {}

    async fn page_up(&self) {}

    async fn page_down(&self) {}

    async fn cursor_up(&self) {}

    async fn cursor_down(&self) {}

    async fn set_surrounding_text(&self, _text: OwnedValue, _cursor_pos: u32, _anchor_pos: u32) {}

    async fn panel_extension_received(&self, _event: OwnedValue) {}

    async fn panel_extension_register_keys(&self, _data: OwnedValue) {}

    #[dbus_interface(signal)]
    async fn commit_text(ctxt: &SignalContext<'_>, text: OwnedValue) -> zbus::Result<()>;
}

impl IbusEngine {
    async fn flush_pending_commit(&self, ctxt: &SignalContext<'_>) -> fdo::Result<()> {
        let pending = {
            let mut guard = self.inner.pending_commit.lock().await;
            guard.take()
        };

        let Some(pending) = pending else {
            return Ok(());
        };

        let serialized = serialize_text(&pending.text);
        Self::commit_text(ctxt, serialized).await.map_err(|err| {
            fdo::Error::Failed(format!("failed to emit SayWrite CommitText: {err}"))
        })?;

        eprintln!(
            "IBus: committed {} bytes, restoring engine '{}'",
            pending.text.len(),
            pending.previous_engine
        );

        if let Some(connection) = self.inner.connection.lock().await.clone() {
            let previous_engine = pending.previous_engine.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(120)).await;
                if let Err(err) = restore_previous_engine(&connection, &previous_engine).await {
                    eprintln!(
                        "IBus: failed to restore engine '{}': {err:#}",
                        previous_engine
                    );
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    if let Err(err) = restore_previous_engine(&connection, &previous_engine).await {
                        eprintln!(
                            "IBus: retry to restore engine '{}' also failed: {err:#}",
                            previous_engine
                        );
                    }
                }
            });
        }

        let _ = pending
            .response
            .send(Ok("Text committed through the SayWrite IBus engine.".into()));
        Ok(())
    }
}

async fn register_component(connection: &Connection) -> Result<()> {
    let proxy = Proxy::new(connection, IBUS_DEST, IBUS_PATH, IBUS_IFACE)
        .await
        .context("failed to create IBus registry proxy")?;
    proxy
        .call_method("RegisterComponent", &(serialize_component(),))
        .await
        .context("failed to register the SayWrite IBus component")?;
    Ok(())
}

async fn restore_previous_engine(connection: &Connection, previous_engine: &str) -> Result<()> {
    let proxy = Proxy::new(connection, IBUS_DEST, IBUS_PATH, IBUS_IFACE)
        .await
        .context("failed to create an IBus proxy for engine restore")?;
    proxy
        .call_method("SetGlobalEngine", &(previous_engine,))
        .await
        .with_context(|| {
            format!("failed to restore the previous IBus engine: {previous_engine}")
        })?;
    Ok(())
}

fn serialize_component() -> OwnedValue {
    let props: HashMap<String, OwnedValue> = HashMap::new();
    let observed_paths: Vec<OwnedValue> = Vec::new();
    let engines = vec![serialize_engine_desc()];
    let command_line = env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "saywrite".into());
    let structure = StructureBuilder::new()
        .add_field("IBusComponent")
        .add_field(props)
        .add_field(COMPONENT_NAME)
        .add_field("SayWrite Dictation Engine")
        .add_field(env!("CARGO_PKG_VERSION"))
        .add_field("MIT")
        .add_field("Fabio")
        .add_field("https://github.com/fabio/saywrite")
        .add_field(command_line)
        .add_field("saywrite")
        .add_field(observed_paths)
        .add_field(engines)
        .build();
    OwnedValue::from(structure)
}

fn serialize_engine_desc() -> OwnedValue {
    let props: HashMap<String, OwnedValue> = HashMap::new();
    let structure = StructureBuilder::new()
        .add_field("IBusEngineDesc")
        .add_field(props)
        .add_field(ENGINE_NAME)
        .add_field("SayWrite")
        .add_field("SayWrite Dictation")
        .add_field("en")
        .add_field("MIT")
        .add_field("Fabio")
        .add_field("")
        .add_field("us")
        .add_field(0_u32)
        .add_field("")
        .add_field("SW")
        .add_field("")
        .add_field("")
        .add_field("")
        .add_field("")
        .add_field("")
        .add_field("")
        .build();
    OwnedValue::from(structure)
}

fn serialize_text(text: &str) -> OwnedValue {
    let props: HashMap<String, OwnedValue> = HashMap::new();
    let attr_props: HashMap<String, OwnedValue> = HashMap::new();
    let attr_list = OwnedValue::from(Structure::from((
        "IBusAttrList",
        attr_props,
        Vec::<OwnedValue>::new(),
    )));

    OwnedValue::from(Structure::from((
        "IBusText",
        props,
        text.to_string(),
        attr_list,
    )))
}

fn parse_object_path(text: &str) -> Option<String> {
    let marker = "objectpath '";
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

fn parse_engine_name(text: &str) -> Option<String> {
    let marker = "IBusEngineDesc',";
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let quote = rest.find('\'')?;
    let rest = &rest[quote + 1..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_engine_name, parse_object_path};

    #[test]
    fn parses_object_path_from_gdbus_output() {
        let output = "(objectpath '/org/freedesktop/IBus/InputContext_2',)";
        assert_eq!(
            parse_object_path(output).as_deref(),
            Some("/org/freedesktop/IBus/InputContext_2")
        );
    }

    #[test]
    fn parses_engine_name_from_gdbus_output() {
        let output = "(<'IBusEngineDesc', {}, 'xkb:us::eng', 'English (US)', '', 'en', '', '', '', uint32 0, '', '', '', '', '', '', '', ''>,)";
        assert_eq!(parse_engine_name(output).as_deref(), Some("xkb:us::eng"));
    }

    #[test]
    fn parses_outputs_with_extra_whitespace() {
        let object_output =
            "  random prefix (objectpath '/org/freedesktop/IBus/InputContext_42',)  ";
        let engine_output =
            "  (< 'IBusEngineDesc', {},   'xkb:gb::eng', 'English (UK)', '', 'en_GB', '', '', '', uint32 0, '', '', '', '', '', '', '', ''>,)  ";
        assert_eq!(
            parse_object_path(object_output).as_deref(),
            Some("/org/freedesktop/IBus/InputContext_42")
        );
        assert_eq!(
            parse_engine_name(engine_output).as_deref(),
            Some("xkb:gb::eng")
        );
    }

    #[test]
    fn parses_high_numbered_input_context_paths() {
        let output = "(objectpath '/org/freedesktop/IBus/InputContext_184467',)";
        assert_eq!(
            parse_object_path(output).as_deref(),
            Some("/org/freedesktop/IBus/InputContext_184467")
        );
    }

    #[test]
    fn parses_engine_names_with_unusual_locale_identifiers() {
        let output = "(<'IBusEngineDesc', {}, 'typing-booster:sv_SE.UTF-8', 'Swedish', '', 'sv_SE.UTF-8', '', '', '', uint32 0, '', '', '', '', '', '', '', ''>,)";
        assert_eq!(
            parse_engine_name(output).as_deref(),
            Some("typing-booster:sv_SE.UTF-8")
        );
    }

    #[test]
    fn returns_none_for_unexpected_gdbus_output() {
        assert_eq!(parse_object_path("()"), None);
        assert_eq!(parse_engine_name("()"), None);
        assert_eq!(parse_object_path(""), None);
        assert_eq!(parse_engine_name(""), None);
    }
}

// ===========================================================================
// Hotkey / GlobalShortcuts portal
// ===========================================================================

static PORTAL_ACTIVE: AtomicBool = AtomicBool::new(false);
const TOGGLE_SHORTCUT_ID: &str = "toggle-dictation";
static TOGGLE_HANDLER: OnceLock<Arc<dyn Fn() + Send + Sync>> = OnceLock::new();

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

    if gnome_desktop() {
        if gnome_shortcut_active(&shortcut) {
            return HotkeyStatus {
                active: true,
                message: format!("GNOME shortcut fallback active for {}.", shortcut),
                setup_hint: String::new(),
            };
        }

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

fn gnome_shortcut_active(shortcut: &str) -> bool {
    let command = gsettings_get(
        "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite-hands-free/",
        "command",
    );
    let binding = gsettings_get(
        "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite-hands-free/",
        "binding",
    );

    let Some(command) = command else {
        return false;
    };
    let Some(binding) = binding else {
        return false;
    };

    let command_ready = command == "/usr/bin/saywrite-dictation.sh"
        || command.ends_with("/scripts/run-global-dictation.sh");
    command_ready && binding == shortcut_to_gnome_binding(shortcut)
}

fn gsettings_get(schema_key: &str, field: &str) -> Option<String> {
    let output = Command::new("gsettings")
        .args(["get", schema_key, field])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .trim_matches('\'')
            .to_string(),
    )
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

#[allow(dead_code)]
pub fn set_toggle_handler(handler: Arc<dyn Fn() + Send + Sync>) {
    let _ = TOGGLE_HANDLER.set(handler);
}

/// Register a global shortcut via the XDG GlobalShortcuts portal and listen
/// for activations. On each activation, toggles the in-process dictation
/// controller. The D-Bus fallback is kept only for legacy launcher commands.
pub async fn register_and_listen() -> Result<()> {
    let conn = Connection::session()
        .await
        .context("no D-Bus session bus")?;

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
    bind_shortcuts(
        &conn,
        &portal,
        &session_handle,
        &settings.global_shortcut_label,
    )
    .await?;

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
            if let Some(handler) = TOGGLE_HANDLER.get() {
                handler();
            } else {
                toggle_dictation_via_compat_dbus(&conn).await;
            }
        }
    }

    PORTAL_ACTIVE.store(false, Ordering::Relaxed);
    Ok(())
}

fn portal_request_path(conn: &Connection, token: &str) -> Result<String> {
    let unique_name = conn
        .unique_name()
        .ok_or_else(|| anyhow!("no unique bus name"))?
        .as_str()
        .replace('.', "_")
        .trim_start_matches(':')
        .to_string();
    Ok(format!(
        "/org/freedesktop/portal/desktop/request/{unique_name}/{token}"
    ))
}

async fn create_session(conn: &Connection, portal: &Proxy<'_>) -> Result<ObjectPath<'static>> {
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    let token = format!("saywrite_{}", std::process::id());
    let session_token = format!("saywrite_session_{}", std::process::id());
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("session_handle_token", Value::from(session_token.as_str()));

    let request_path = portal_request_path(conn, &token)?;

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

    let request_path = portal_request_path(conn, &token)?;
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

            if path.as_deref() == Some(request_path)
                && interface.as_deref() == Some("org.freedesktop.portal.Request")
                && member.as_deref() == Some("Response")
            {
                let body: (u32, HashMap<String, OwnedValue>) =
                    msg.body().context("failed to parse Response body")?;
                let (response_code, results) = body;
                if response_code != 0 {
                    return Err(anyhow!(
                        "Portal request denied (response code {})",
                        response_code
                    ));
                }
                if let Some(session_val) = results.get("session_handle") {
                    let session_str: String = session_val.clone().try_into().unwrap_or_default();
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
        Err(anyhow!(
            "D-Bus stream ended while waiting for portal response"
        ))
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(anyhow!("Timed out waiting for portal response")),
    }
}

async fn toggle_dictation_via_compat_dbus(conn: &Connection) {
    let proxy = match Proxy::new(
        conn,
        crate::integration_api::COMPAT_BUS_NAME,
        crate::integration_api::COMPAT_OBJECT_PATH,
        crate::integration_api::COMPAT_INTERFACE_NAME,
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to create compatibility proxy for toggle: {e}");
            return;
        }
    };

    match proxy.call_method("ToggleDictation", &()).await {
        Ok(_) => {}
        Err(e) => eprintln!("ToggleDictation call failed: {e}"),
    }
}

fn portal_interface_available() -> bool {
    Command::new("busctl")
        .args([
            "--user",
            "introspect",
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
        ])
        .output()
        .map(|r| {
            r.status.success()
                && String::from_utf8_lossy(&r.stdout)
                    .contains("org.freedesktop.portal.GlobalShortcuts")
        })
        .unwrap_or(false)
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
