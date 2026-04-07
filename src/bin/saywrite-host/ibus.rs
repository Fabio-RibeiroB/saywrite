use std::{
    collections::HashMap,
    env,
    process::Command,
    sync::{Arc, OnceLock},
    thread,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use tokio::{
    sync::{oneshot, Mutex},
    time::timeout,
};
use zbus::{
    dbus_interface, fdo,
    zvariant::{OwnedObjectPath, OwnedValue, Structure, StructureBuilder},
    Connection, ConnectionBuilder, Proxy, SignalContext,
};

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
    env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
}

pub fn gnome_wayland() -> bool {
    let session = env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    session.eq_ignore_ascii_case("wayland") && desktop.to_ascii_lowercase().contains("gnome")
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
    let command_line = format!(
        "{}/.local/bin/saywrite-host --ibus-engine",
        dirs::home_dir()
            .and_then(|p| p.to_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "~".into())
    );
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
        let object_output = "  random prefix (objectpath '/org/freedesktop/IBus/InputContext_42',)  ";
        let engine_output =
            "  (< 'IBusEngineDesc', {},   'xkb:gb::eng', 'English (UK)', '', 'en_GB', '', '', '', uint32 0, '', '', '', '', '', '', '', ''>,)  ";
        assert_eq!(
            parse_object_path(object_output).as_deref(),
            Some("/org/freedesktop/IBus/InputContext_42")
        );
        assert_eq!(parse_engine_name(engine_output).as_deref(), Some("xkb:gb::eng"));
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
