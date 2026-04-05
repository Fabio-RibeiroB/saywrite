# SayWrite Implementation Plan

## Current State (April 2025)

Pure Rust GTK4/libadwaita app with a companion host daemon. No Python. Both binaries compile, clippy clean, 9 tests pass.

### App (`saywrite`)

- **`src/config.rs`** — `AppSettings`, `ProviderMode` enum, XDG-compliant paths
- **`src/cleanup.rs`** — deterministic transcript cleanup (filler removal, spoken punctuation, capitalization). 9 passing tests.
- **`src/dictation.rs`** — GStreamer mic capture via `gst-launch-1.0`, whisper.cpp CLI transcription, thread-safe session state
- **`src/model_installer.rs`** — downloads `ggml-base.en.bin` from Hugging Face with progress callbacks, atomic write, size validation
- **`src/host_integration.rs`** — D-Bus client (tries `InsertText` via zbus first), Unix socket fallback, clipboard last resort
- **`src/host_api.rs`** — shared D-Bus contract constants (bus name, object path, state strings)
- **`src/runtime.rs`** — readiness probing (model, whisper CLI, host availability, acceleration)
- **`src/ui/onboarding.rs`** — 4-step carousel: welcome → mic test (real GStreamer capture) → shortcut → engine (inline model download with progress bar)
- **`src/ui/main_window.rs`** — dictation window (640×560), readiness-gated Start button
- **`src/ui/preferences.rs`** — settings with model download button, live host status display

### Host daemon (`saywrite-host`)

- **`src/bin/saywrite-host/main.rs`** — long-lived daemon, graceful shutdown on SIGINT/SIGTERM
- **`src/bin/saywrite-host/dbus.rs`** — owns `io.github.saywrite.Host` on session bus; implements `GetStatus`, `InsertText`, `ToggleDictation`; emits `DictationStateChanged`, `TextReady`, `InsertionResult` signals
- **`src/bin/saywrite-host/insertion.rs`** — auto-detects `wtype`, `xdotool`, `wl-copy`, `xclip`, `xsel`; clipboard fallback
- **`src/bin/saywrite-host/hotkey.rs`** — detects GlobalShortcuts portal presence, prints `busctl` fallback instructions

### Packaging

- **`flatpak/io.github.fabio.SayWrite.json`** — builds whisper.cpp from vendored source, installs `whisper-cli` to `/app/bin/`, grants mic + network + D-Bus permissions
- **`data/`** — desktop file, AppStream metainfo, D-Bus activation file, systemd user service

---

## What's Next

### 1. Flatpak end-to-end verification

The manifest is written but hasn't been tested in a real `flatpak-builder` run.

- Run `flatpak-builder --force-clean build flatpak/io.github.fabio.SayWrite.json`
- Fix any build failures (missing SDK extensions, whisper.cpp cmake flags, GStreamer plugin availability in sandbox)
- Verify: app launches in sandbox, mic capture works, `whisper-cli` is at `/app/bin/whisper-cli`
- Verify: model download works from inside sandbox (needs `--share=network`)
- Verify: D-Bus talk-name permission allows communication with host daemon running outside sandbox

### 2. GlobalShortcuts portal registration

`src/bin/saywrite-host/hotkey.rs` currently detects portal presence but doesn't register a shortcut.

- Add `ashpd` crate dependency
- Implement `org.freedesktop.portal.GlobalShortcuts` registration
  - `CreateSession` → `BindShortcuts` with the configured key combo
  - Listen for `Activated` signal → call `ToggleDictation` internally
- Handle portal unavailability gracefully (already prints fallback instructions)
- Test on GNOME 44+ and KDE Plasma 5.27+

### 3. IBus text insertion

`src/bin/saywrite-host/insertion.rs` currently uses command-based backends (`wtype`, `xdotool`). IBus is the correct primary path for Wayland.

- Connect to IBus bus via D-Bus (`org.freedesktop.IBus`)
- Get current input context
- Commit text string directly into the focused field
- Fall back to existing command-based backends if IBus unavailable
- Report active insertion method through `GetStatus`

### 4. App ↔ Host signal handling

The app can call host methods via D-Bus but doesn't listen for signals yet.

- Subscribe to `DictationStateChanged` signal in `host_integration.rs`
- When host triggers dictation via hotkey, update main window UI (state label, button style) in real time
- Subscribe to `TextReady` signal to show transcript in the app even when dictation was started from the host
- This requires bridging async zbus signal streams into the GLib main loop

### 5. Cloud provider wiring

`src/dictation.rs` currently rejects `ProviderMode::Cloud` with an error message. Wire it up.

- Add an HTTP transcription path (OpenAI-compatible `/v1/audio/transcriptions` endpoint)
- Use `ureq` (already a dependency) to POST the WAV file with the configured API key
- Parse response, run through `cleanup_transcript`, return `TranscriptResult`
- Skip whisper-cli and model checks when in Cloud mode
- Update readiness gating: Cloud mode only needs mic + API key configured

### 6. Host daemon packaging & install UX

Users need a way to install `saywrite-host` outside the Flatpak sandbox.

- Decide on distribution: `.deb`/`.rpm`, AUR package, or a simple install script
- The systemd user service (`data/saywrite-host.service`) should auto-start the daemon on login
- D-Bus activation file (`data/io.github.saywrite.Host.service`) should auto-start daemon on first method call
- Add an in-app status indicator: "Host companion: connected / not installed"
- Consider a "Install host companion" button in preferences that explains the steps

### 7. Polish pass

- CSS transitions for state changes (spinner fade, transcript slide-in)
- Keyboard navigation: Enter to start/stop dictation, Escape to dismiss
- Dark mode support (current CSS uses hardcoded light colors)
- Model selection: offer multiple whisper model sizes (tiny, base, small, medium)
- Download resume: if partial `.part` file exists and server supports Range, resume instead of restart

---

## D-Bus Interface Reference

**Bus name:** `io.github.saywrite.Host`
**Object path:** `/io/github/saywrite/Host`

| Method | Args | Returns | Description |
|--------|------|---------|-------------|
| `GetStatus` | — | `(s status, b hotkey_active, b insertion_available)` | Host readiness |
| `InsertText` | `(s text)` | `(b ok, s message)` | Type text into focused app |
| `ToggleDictation` | — | `(b ok, s message)` | Start or stop dictation |

| Signal | Args | Description |
|--------|------|-------------|
| `DictationStateChanged` | `(s state)` | `idle`, `listening`, `processing`, `done` |
| `TextReady` | `(s cleaned_text, s raw_text)` | Final transcript available |
| `InsertionResult` | `(b ok, s message)` | Result of text insertion |

## Risks

- **GlobalShortcuts portal coverage:** Not universal across desktops. The `busctl` fallback must be documented clearly.
- **IBus availability:** Not all desktops run IBus. The command-based fallback chain (`wtype` → `xdotool` → clipboard) must stay robust.
- **Flatpak ↔ host D-Bus boundary:** Permissions must be correct for the sandboxed app to talk to the native daemon.
- **Model download size:** `ggml-base.en.bin` is ~142 MB. Resume support and clear progress are important.
- **Wayland input restrictions:** Traditional keystroke injection doesn't work. IBus and portals are the sanctioned paths.

## Success Criteria

1. `flatpak install` → onboarding → model downloads → local dictation works standalone
2. `saywrite-host` running → global hotkey → dictation → text appears in focused app
3. Without host: app works fully via clipboard fallback
4. Cloud mode works with an OpenAI-compatible API key
5. `cargo test` passes, `cargo clippy` clean, no Python anywhere
