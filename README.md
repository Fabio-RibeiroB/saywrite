# SayWrite

SayWrite is a Linux dictation app. Press a hotkey, speak, and your words land in the active text field — cleaned up and ready to use.

Install through Flatpak. Use it right away with clipboard delivery. Optionally enable direct typing for deeper system integration.

> **Early work in progress.** SayWrite is still under active development. It works well on the setups it has been tested on, but it may not work for you yet. Direct typing support is desktop-dependent, and not every Linux environment is validated. If you try it and something breaks, that feedback is welcome.

## How It Works for Users

SayWrite has two user-facing modes:

### Clipboard Mode

Works with the Flatpak app alone. No host setup required.

- press the dictation hotkey
- speak
- SayWrite records, transcribes, cleans up your text, and copies it to the clipboard
- paste into any application

This is the default mode and works on any desktop where the Flatpak runs.

### Direct Typing Mode

Requires the host companion (`saywrite-host`) installed alongside the Flatpak.

- press the dictation hotkey
- speak
- SayWrite types the cleaned text directly into the focused application

Direct typing is hotkey-driven while SayWrite is running — you do not need to keep the window focused. Closing the app disarms the host companion so dictation does not keep running in the background.

You can replay onboarding from Settings at any time if you want to re-check microphone, hotkey, or mode setup.

## Current Support Status

| Environment | Direct Typing | Clipboard Mode |
|---|---|---|
| GNOME Wayland + IBus | Supported | Supported |
| X11 + xdotool | Untested | Supported |
| wlroots Wayland + wtype | Untested | Supported |
| Other Wayland compositors | Not available | Supported |

**Supported** means tested end-to-end on real hardware. **Untested** means the code path exists but has not yet been validated. Clipboard Mode works everywhere the Flatpak runs.

Do not expect universal direct typing support across all Linux desktops yet. The GNOME Wayland path (via IBus) is the current validated path. More environments will be confirmed as testing expands.

## Current Product Model

```
┌─────────────────────────────┐
│  Flatpak app (GTK/Adwaita)  │  ← install via Flatpak / Flathub
│  settings · diagnostics     │
│  transcript preview         │
└────────────┬────────────────┘
             │
             │ Clipboard Mode (default)
             │   transcript → clipboard → paste anywhere
             │
             │ Direct Typing Mode (optional host companion)
             │   transcript → saywrite-host → IBus → active text field
             │
             ▼
┌─────────────────────────────┐
│  saywrite-host (native)     │  ← installed outside Flatpak sandbox
│  IBus engine · fallbacks    │
└─────────────────────────────┘
```

The Flatpak sandbox cannot inject keystrokes into arbitrary host applications. The host companion runs outside that boundary and handles text insertion. This is an intentional design, not a workaround: the Flatpak handles discovery, onboarding, and settings; the host companion handles system-wide input.

## Getting Started

SayWrite is distributed as a Flatpak. It is not yet on Flathub, but you can install it directly from a GitHub release or by building locally.

### Option 1: Install from a GitHub Release (Recommended)

Each release ships a pre-built `.flatpak` bundle. Grab the latest release from the [Releases page](https://github.com/Fabio-RibeiroB/saywrite/releases) and install it:

```bash
# Download the latest Flatpak bundle
curl -L -o saywrite.flatpak \
  "https://github.com/Fabio-RibeiroB/saywrite/releases/latest/download/saywrite-x86_64.flatpak"

# Install it (this also pulls in the GNOME 48 runtime if needed)
flatpak install --user ./saywrite.flatpak

# Run it
flatpak run io.github.fabio.SayWrite
```

You can also install a specific version by replacing `latest` with a tag, for example `download/v0.3.0/saywrite-x86_64.flatpak`.

### Option 2: Build and Install from Source

If you prefer to build from the repository:

```bash
# Clone and build
git clone https://github.com/Fabio-RibeiroB/saywrite.git
cd saywrite
flatpak-builder --user --install --force-clean build-dir flatpak/io.github.fabio.SayWrite.json

# Run it
flatpak run io.github.fabio.SayWrite
```

### After First Launch

1. Complete the onboarding carousel to set up your microphone and dictation shortcut.
2. Choose **Local** (whisper.cpp) or **Cloud** (OpenAI-compatible API) as your transcription provider.
3. For **Direct Typing Mode**, install the host companion from Settings — this enables hotkey-driven dictation with text inserted directly into the focused application.
4. Without the host companion, **Clipboard Mode** works immediately: dictation copies cleaned text to your clipboard for you to paste anywhere.

## Why SayWrite

Current Linux dictation options tend to fail in one of three ways:

- good engine, bad UX
- good UX, weak system integration
- powerful setup, hostile onboarding

SayWrite takes the opposite approach: opinionated defaults, polished UI, and system integration designed from the start.

## Developer Setup

> **Note:** This section is for contributors building from source. It is not the end-user install flow.

### Prerequisites (Ubuntu-like systems)

Install Rust/GTK development dependencies:

```bash
./scripts/bootstrap-rust-dev.sh
```

Install native host dependencies:

```bash
./scripts/bootstrap-dev.sh
```

### Build and Run

```bash
cargo run
```

### Local Whisper Backend

To use local transcription, build and set up `whisper.cpp`:

```bash
./scripts/setup-whispercpp.sh
./scripts/download-local-model.sh
```

### Host Companion (Direct Typing Mode)

Build and install `saywrite-host`:

```bash
cargo build --release
./scripts/install-host.sh
```

The install script requires the release binary at `target/release/saywrite-host`. The `--release` flag is required; a debug build will not satisfy this path. The script installs the companion daemon and sets up the systemd user service and D-Bus activation.

### GNOME Shortcut (Developer Fallback)

If the app's shortcut registration does not work on your dev setup, you can bind the hotkey manually:

```bash
chmod +x ./scripts/run-global-dictation.sh ./scripts/install-gnome-shortcut.sh
./scripts/install-gnome-shortcut.sh
```

This binds `Super+Alt+D` to trigger dictation via D-Bus.

## Repository Layout

```
src/                        Rust app source
  bin/saywrite-host/        Host companion daemon (dbus, input, insertion, service)
  ui/                       GTK/libadwaita UI components
    main_window/            Main dictation window
    onboarding.rs           Onboarding carousel
    preferences.rs          Settings and diagnostics
    shortcut_capture.rs     Keyboard shortcut capture dialog
    async_poll.rs           GTK-safe background task polling
  config.rs                 AppSettings, ProviderMode, ModelSize, JSON load/save
  cleanup.rs                Transcript cleanup rules
  dictation.rs              Mic capture, whisper transcription, cloud handoff
  host_api.rs               D-Bus constants and host status types
  host_integration.rs       D-Bus client for host communication
  host_setup.rs             Host install flow and desktop detection
  model_installer.rs        Model download and cache management
  runtime.rs                Capability probing (GPU, whisper, insertion)
data/                       Desktop metadata and icons
flatpak/                    Flatpak manifest
scripts/                    Developer and installation scripts (see scripts/README.md)
docs/                       Product and architecture documentation (see docs/README.md)
vendor/                     Vendored dependencies (whisper.cpp)
```

## Documentation

See [docs/README.md](docs/README.md) for the documentation index, including which docs are current versus historical.

Key docs:
- [docs/next_steps.md](docs/next_steps.md) — active product and engineering priorities
- [docs/support_matrix.md](docs/support_matrix.md) — release validation and supported environments
- [docs/holistic_review.md](docs/holistic_review.md) — current technical assessment
- [docs/architecture.md](docs/architecture.md) — design rationale (historical; not the current implementation plan)

## Current Implementation Status

The current app and host workflow are Rust-native. The supported development path is the GTK app plus the `saywrite-host` daemon described above.

Current state:
- GTK app with onboarding, main dictation window, settings, and diagnostics
- `saywrite-host` owns the real dictation workflow (D-Bus service, IBus bridge, GlobalShortcuts portal)
- Host daemon lifecycle tied to GUI (starts on app launch, stops and is masked on close)
- Global hotkey dictation works through the host path while SayWrite is running
- Local (whisper.cpp) and cloud (OpenAI-compatible API) transcription both work end-to-end
- Direct insertion works on the validated GNOME Wayland setup via IBus bridge
- `wtype` (Wayland) and `xdotool` (X11) insertion paths exist but are untested on real hardware
- Clipboard and notification fallbacks work on other environments
- Desktop detection auto-selects the best insertion backend per session
- In-app host installation with progress feedback
- Shortcut capture dialog with GNOME keybinding suspend/restore
- Host-side unit tests cover backend classification, result-kind mapping, IBus parsing, error sanitization, and toggle debounce

The next major milestone is cross-desktop validation and release polish so Direct Typing Mode can be documented with narrower, evidence-backed claims.
