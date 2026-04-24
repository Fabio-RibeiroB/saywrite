# AGENTS.md — SayWrite Codebase Guide

SayWrite is a Linux-first dictation app built with Rust. The goal is simple: speak, get cleaned polished text delivered back to the active app, with no external setup docs.

## Design Philosophy

Code should be written smartly. The codebase should be easy to read with re-used and maintainable code.
The app should be sleek, modern and easy to use. Using it should have that fun-feel and be well designed.
It should look and feel beautiful. If someone gets stuck setting it up or using it then it's badly designed.

## Stack

- **Language**: Rust 2021
- **UI**: Currently GTK4 + libadwaita but should be Tauri
- **Audio**: GStreamer 1.0
- **ASR**: whisper.cpp (local), configurable cloud API (OpenAI-compatible)
- **IPC**: D-Bus via `zbus` for compatibility paths and desktop integration
- **Packaging**: native `.deb`-first on the `deb-first` branch

## Current Layout

| File                                | Role                                                                                    |
| ----------------------------------- | --------------------------------------------------------------------------------------- |
| `src/main.rs`                       | GTK application entry point                                                             |
| `src/app.rs`                        | GTK application + activation flow                                                       |
| `src/lib.rs`                        | shared library surface                                                                  |
| `src/config.rs`                     | `AppSettings`, `ProviderMode`, `ModelSize`, JSON load/save, XDG paths                   |
| `src/cleanup.rs`                    | `cleanup_transcript()`                                                                  |
| `src/dictation.rs`                  | microphone session control, whisper.cpp CLI transcription, cloud handoff, session state |
| `src/model_installer.rs`            | model download, validation, and cache flow                                              |
| `src/native_integration.rs`         | in-process direct-typing integration + compatibility D-Bus adapter                      |
| `src/integration_api.rs`            | runtime status/capability vocabulary + legacy D-Bus constants                           |
| `src/desktop_setup.rs`              | desktop detection, diagnostics, legacy cleanup, GNOME shortcut helpers                  |
| `src/input.rs`                      | shared hotkey + IBus integration                                                        |
| `src/insertion.rs`                  | shared desktop insertion backends                                                       |
| `src/service.rs`                    | shared dictation/insertion controller                                                   |
| `src/runtime.rs`                    | readiness probing (GPU, whisper, insertion)                                             |
| `src/ui/main_window/`               | main dictation window                                                                   |
| `src/ui/async_poll.rs`              | GTK-safe background task polling helper                                                 |
| `src/ui/onboarding.rs`              | onboarding carousel                                                                     |
| `src/ui/preferences.rs`             | preferences and diagnostics                                                             |
| `src/ui/shortcut_capture.rs`        | keyboard shortcut capture dialog                                                        |
| `scripts/install-gnome-shortcut.sh` | GNOME custom shortcut fallback installer                                                |

## User-Facing Modes

The product exposes two modes to users:

- **Clipboard Mode** — records, transcribes, cleans, and copies text to the clipboard. Default and always available.
- **Direct Typing Mode** — text is inserted directly into the focused application via IBus (GNOME Wayland) or other backends when the current desktop supports it.

When writing copy, diagnostics, or onboarding text, use these mode names. Do not expose internal terms like "IBus bridge" or "D-Bus path" to users.

## Key Facts

- `AppSettings.provider_mode` is an enum: `Local` or `Cloud`.
- `AppSettings.local_model_path` is optional and stored as a `PathBuf`.
- `AppSettings.model_size` is an enum: `Tiny`, `Base`, or `Small`.
- `AppSettings.global_shortcut_label` defaults to `Super+Alt+D`.
- `cleanup_transcript()` is deterministic and should stay conservative.
- The app exposes `io.github.saywrite.Host` itself as a compatibility D-Bus interface.
- The primary direct-typing path is in-process; there is no Unix socket path.
- The app attempts XDG GlobalShortcuts portal registration at startup.
- Insertion exposes explicit capability/result categories: `typing`/`clipboard-only`/`notification-only`/`unavailable` and `typed`/`copied`/`notified`/`failed`.
- On GNOME Wayland, insertion prefers the SayWrite IBus engine bridge; on other setups it falls back to `wtype`, `xdotool`, clipboard tools, or notifications.
- Shortcut capture suspends GNOME keybindings during capture and restores them afterward.
- Toggle debounce (900ms) prevents repeated shortcut activations from wedging dictation state.
- Settings can replay onboarding without wiping the rest of the app state.

## Current State

- GTK UI with onboarding carousel, main dictation window, settings, diagnostics, and shortcut capture.
- Microphone capture uses GStreamer with WirePlumber default source detection and silence filtering.
- Cleanup produces deterministic cleaned text.
- Local transcription works if `whisper-cli` and a model are installed.
- Cloud mode is wired through the configured OpenAI-compatible API base and key.
- The app owns the direct-typing controller and compatibility D-Bus interface.
- The compatibility interface handles `GetStatus`, `InsertText`, and `ToggleDictation`.
- XDG GlobalShortcuts portal registration is implemented and works on KDE and wlroots compositors.
- The SayWrite IBus bridge is the primary GNOME Wayland insertion path with engine swap/restore and retry logic.
- Desktop detection (GNOME Wayland, Other Wayland, X11, Other) with per-profile dependency checks and package hints.
- Unit tests cover IBus parsing, insertion backend classification, result-kind mapping, error sanitization, and toggle debounce.

## Design Principles

1. Simple and clean beats clever and fragile.
2. Setup lives inside the app, not in a README.
3. Cleanup is the product differentiator.
4. Local-first, offline-capable after model download.
5. Auto-detect acceleration.
6. Native-first; keep the runtime model simple.
7. Make changes with the user base in mind. Don't just get it working on the developer's PC. Before marking work done, ask: "would this still work on a machine that only installed the native package or built from source on a supported distro?" If the answer is no or "not sure", fix that gap before shipping.

## Repo Hygiene

- Keep `README.md` and `AGENTS.md` up to date whenever product behavior, supported workflows, setup steps, or architecture assumptions change.
- Remove dead compatibility shims, outdated scripts, and stale copy when they no longer reflect the supported native in-process architecture.
- Treat clearly marked historical docs in `docs/` as archival context, not as current source of truth.

## Testing

- Run `cargo test`.
- Run `cargo check` before deeper changes.
- Prefer checking both binaries when touching shared interfaces or compatibility paths.
- No mocking of filesystem or GStreamer when real objects are practical.
- On `deb-first`, validate native behavior with `cargo check`, `cargo test`, and `cargo deb` builds. If the user needs to test an installed package, rebuild the `.deb` and reinstall it before asking them to try anything.

## Docs

- `docs/README.md` — documentation index
- `docs/next_steps.md` — active plan and release priorities
- `docs/support_matrix.md` — release validation and supported-environment claims
