# SayWrite

SayWrite is a Linux dictation app. Press a hotkey, speak, and your words land in the active text field — cleaned up and ready to use.

The `deb-first` branch is the native Debian-first line. Native Debian/Ubuntu/Zorin installs are the primary target, and Direct Typing is now brought up by the app itself on native builds.

> **Early work in progress.** SayWrite is still under active development. It works well on the setups it has been tested on, but it may not work for you yet. Direct typing support is desktop-dependent, and not every Linux environment is validated. If you try it and something breaks, that feedback is welcome.

## How It Works for Users

SayWrite has two user-facing modes:

### Clipboard Mode

Works with the app alone. No extra setup required.

- press the dictation hotkey
- speak
- SayWrite records, transcribes, cleans up your text, and copies it to the clipboard
- paste into any application

This is the default mode and works on the current builds without host setup.

### Direct Typing Mode

Works on supported desktops with no separate companion install.

- press the dictation hotkey
- speak
- SayWrite types the cleaned text directly into the focused application

Direct typing is hotkey-driven while SayWrite is running — you do not need to keep the window focused.

You can replay onboarding from Settings at any time if you want to re-check microphone, hotkey, or mode setup.

## Current Support Status

| Environment | Direct Typing | Clipboard Mode |
|---|---|---|
| GNOME Wayland + IBus | Supported | Supported |
| X11 + xdotool | Untested | Supported |
| wlroots Wayland + wtype | Untested | Supported |
| Other Wayland compositors | Not available | Supported |

**Supported** means tested end-to-end on real hardware. **Untested** means the code path exists but has not yet been validated.

Do not expect universal direct typing support across Linux desktops yet. GNOME Wayland via IBus is the current validated path. More environments will be confirmed only after they are tested.

## Current Product Model

```
┌─────────────────────────────┐
│  Native app (GTK/Adwaita)   │  ← primary target on deb-first
│  settings · diagnostics     │
│  transcript preview         │
└────────────┬────────────────┘
             │
             │ Clipboard Mode (default)
             │   transcript → clipboard → paste anywhere
             │
             │ Direct Typing Mode
             │   transcript → in-process insertion → active text field
```

The native build now owns dictation control, shortcut handling, and direct typing itself. Desktop-specific insertion backends still determine whether true typing is available.

## Getting Started

This branch is moving toward native Debian packaging. Until dedicated `.deb` artifacts are published, the practical dogfooding path is a native source build on Debian-family systems.

### Option 1: Native Build on Debian/Ubuntu/Zorin (Recommended on `deb-first`)

Use this path if you are testing the Debian-first migration. It keeps you on the native runtime model we are moving toward.

```bash
git clone https://github.com/Fabio-RibeiroB/saywrite.git
cd saywrite
cargo run --release
```

Dedicated `.deb` packaging is wired up on this branch, but this README does not claim that published `.deb` release assets already exist.

### After First Launch

1. Complete the onboarding carousel to set up your microphone and dictation shortcut.
2. Choose **Local** (whisper.cpp) or **Cloud** (OpenAI-compatible API) as your transcription provider.
3. **Direct Typing Mode** is available automatically when your desktop session supports it.
4. **Clipboard Mode** remains the fallback when direct typing is unavailable on the current desktop.

## Why SayWrite

Current Linux dictation options tend to fail in one of three ways:

- good engine, bad UX
- good UX, weak system integration
- powerful setup, hostile onboarding

SayWrite takes the opposite approach: opinionated defaults, polished UI, and system integration designed from the start.

## Developer Setup

> **Note:** This section is for contributors building from source. It is not the end-user install flow.

On `deb-first`, native development and native dogfooding are the supported paths.

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

### GNOME Shortcut (Developer Fallback)

If the app's shortcut registration does not work on your dev setup, you can bind the hotkey manually:

```bash
chmod +x ./scripts/run-global-dictation.sh ./scripts/install-gnome-shortcut.sh
./scripts/install-gnome-shortcut.sh
```

This binds `Super+Alt+D` to trigger dictation through the app's compatibility D-Bus interface.

## Repository Layout

```
src/                        Rust app source
  bin/saywrite-host/        Historical standalone daemon target kept during migration
  input.rs                  Shared hotkey + IBus integration
  insertion.rs              Shared desktop insertion backends
  service.rs                Shared dictation/insertion controller
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
  host_integration.rs       In-process direct-typing integration + compatibility D-Bus interface
  host_setup.rs             Desktop detection, diagnostics, and GNOME shortcut helpers
  model_installer.rs        Model download and cache management
  runtime.rs                Capability probing (GPU, whisper, insertion)
data/                       Desktop metadata and icons
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

The current supported runtime on `deb-first` is native and in-process.

Current state:
- GTK app with onboarding, main dictation window, settings, and diagnostics
- The app owns the real dictation workflow, shortcut handling, and insertion orchestration on native builds
- The app exposes a compatibility D-Bus interface so existing GNOME fallback launchers continue to work during migration
- Global hotkey dictation works through the host path while SayWrite is running
- Local (whisper.cpp) and cloud (OpenAI-compatible API) transcription both work end-to-end
- Direct insertion works on the validated GNOME Wayland setup via IBus bridge
- `wtype` (Wayland) and `xdotool` (X11) insertion paths exist but are untested on real hardware
- Clipboard and notification fallbacks work on other environments
- Desktop detection auto-selects the best insertion backend per session
- Shortcut capture dialog with GNOME keybinding suspend/restore
- Host-side unit tests cover backend classification, result-kind mapping, IBus parsing, error sanitization, and toggle debounce

The next major milestone on `deb-first` is cleanup: remove the obsolete standalone daemon packaging leftovers and finish simplifying the remaining migration-era copy and files. The validated direct-typing path remains GNOME Wayland.
