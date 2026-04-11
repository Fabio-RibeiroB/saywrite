# SayWrite

SayWrite is a Linux dictation app. Press a hotkey, speak, and your words land in the active text field — cleaned up and ready to use.

Install through Flatpak. Use it right away with clipboard delivery. Optionally enable direct typing for deeper system integration.

## How It Works for Users

SayWrite has two user-facing modes:

### Clipboard Mode

Works with the Flatpak app alone. No host setup required.

- press the dictation shortcut
- speak
- SayWrite records, transcribes, cleans up your text, and copies it to the clipboard
- paste into any application

This is the default mode and works on any desktop where the Flatpak runs.

### Direct Typing Mode

Requires the host companion (`saywrite-host`) installed alongside the Flatpak.

- press the dictation shortcut
- speak
- SayWrite types the cleaned text directly into the focused application

Direct typing is hotkey-driven — you do not need to keep the app open or switch focus. The app walks you through enabling this mode from its settings screen.

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

## Installing

Flatpak distribution is the planned primary install path. Instructions will be added here when the app reaches a public release.

For now, see [Developer Setup](#developer-setup) below.

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
src/                    Rust app source
  bin/saywrite-host     Host companion daemon
  ui/                   GTK/libadwaita UI components
data/                   Desktop metadata and icons
flatpak/                Flatpak manifest
scripts/                Developer and installation scripts (see scripts/README.md)
docs/                   Product and architecture documentation (see docs/README.md)
vendor/                 Vendored dependencies (whisper.cpp)
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
- GTK app exists as the setup and diagnostics surface
- `saywrite-host` owns the real dictation workflow
- global hotkey dictation works through the host path
- local and cloud transcription both work end-to-end
- direct insertion works on the validated GNOME Wayland setup
- clipboard and notification fallbacks work on other environments

The next major milestone is support-matrix validation and release polish so Direct Typing Mode can be documented with narrower, evidence-backed claims.
