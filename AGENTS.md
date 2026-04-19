# AGENTS.md — SayWrite Codebase Guide

SayWrite is a Linux-first dictation app with a Rust GTK4/libadwaita shell and a Rust host daemon. The goal is simple: speak, get cleaned polished text delivered back to the active app, with no external setup docs.

## Stack

- **Language**: Rust 2021
- **UI**: GTK4 + libadwaita
- **Audio**: GStreamer 1.0
- **ASR**: whisper.cpp (local), configurable cloud API (OpenAI-compatible)
- **IPC**: D-Bus via `zbus`
- **Packaging**: Flatpak app plus native host companion install flow

## Current Layout

| File | Role |
|------|------|
| `src/main.rs` | GTK application entry point |
| `src/app.rs` | GTK application + activation flow, host daemon lifecycle |
| `src/lib.rs` | shared library surface for app and host |
| `src/config.rs` | `AppSettings`, `ProviderMode`, `ModelSize`, JSON load/save, XDG paths |
| `src/cleanup.rs` | `cleanup_transcript()` |
| `src/dictation.rs` | microphone session control, whisper.cpp CLI transcription, cloud handoff, session state |
| `src/model_installer.rs` | model download, validation, and cache flow |
| `src/host_integration.rs` | D-Bus client for host communication, signal subscription |
| `src/host_api.rs` | shared D-Bus constants, host status types, capability/result enums |
| `src/host_setup.rs` | host install flow, desktop detection, diagnostics |
| `src/runtime.rs` | readiness probing (GPU, whisper, insertion) |
| `src/ui/main_window/` | main dictation window |
| `src/ui/async_poll.rs` | GTK-safe background task polling helper |
| `src/ui/onboarding.rs` | onboarding carousel |
| `src/ui/preferences.rs` | preferences and diagnostics |
| `src/ui/shortcut_capture.rs` | keyboard shortcut capture dialog |
| `src/bin/saywrite-host/` | host daemon: `main.rs`, `dbus.rs`, `input.rs` (IBus bridge, GlobalShortcuts portal), `insertion.rs`, `service.rs` |
| `scripts/install-host.sh` | installs `saywrite-host` and `whisper-cli` as user services + D-Bus service |
| `scripts/install-gnome-shortcut.sh` | GNOME custom shortcut fallback installer |

## User-Facing Modes

The product exposes two modes to users:

- **Clipboard Mode** — works with the Flatpak app alone; records, transcribes, cleans, and copies text to the clipboard. Default and always available.
- **Direct Typing Mode** — requires `saywrite-host` installed outside the sandbox; text is inserted directly into the focused application via IBus (GNOME Wayland) or other backends.

When writing copy, diagnostics, or onboarding text, use these mode names. Do not expose internal terms like "IBus bridge" or "D-Bus path" to users.

## Key Facts

- `AppSettings.provider_mode` is an enum: `Local` or `Cloud`.
- `AppSettings.local_model_path` is optional and stored as a `PathBuf`.
- `AppSettings.model_size` is an enum: `Tiny`, `Base`, or `Small`.
- `AppSettings.global_shortcut_label` defaults to `Super+Alt+D`.
- `cleanup_transcript()` is deterministic and should stay conservative.
- `saywrite-host` exists and answers D-Bus calls on `io.github.saywrite.Host`.
- The app communicates with the host via D-Bus; there is no Unix socket path.
- The host attempts XDG GlobalShortcuts portal registration at startup and reports status over D-Bus.
- Host insertion exposes explicit capability/result categories: `typing`/`clipboard-only`/`notification-only`/`unavailable` and `typed`/`copied`/`notified`/`failed`.
- On GNOME Wayland, host insertion prefers the SayWrite IBus engine bridge; on other setups it falls back to `wtype`, `xdotool`, clipboard tools, or notifications.
- The GUI starts `saywrite-host` on launch and stops + masks it on app shutdown.
- `saywrite-host` refuses to start unless the app owns `io.github.fabio.SayWrite` on the session bus, preventing orphan daemons.
- Settings sync from Flatpak to host via `flatpak-spawn --host` on save.
- Shortcut capture suspends GNOME keybindings during capture and restores them afterward.
- Host-side toggle debounce (900ms) prevents repeated shortcut activations from wedging the daemon.
- Settings can replay onboarding without wiping the rest of the app state.

## Current State

- GTK UI with onboarding carousel, main dictation window, settings, diagnostics, and shortcut capture.
- Microphone capture uses GStreamer with WirePlumber default source detection and silence filtering.
- Cleanup produces deterministic cleaned text.
- Local transcription works if `whisper-cli` and a model are installed.
- Cloud mode is wired through the configured OpenAI-compatible API base and key.
- `saywrite-host` starts, owns the D-Bus name, and handles `GetStatus`, `InsertText`, and `ToggleDictation`.
- The host emits D-Bus signals for dictation state, ready text, and insertion results (with `result_kind`).
- XDG GlobalShortcuts portal registration is implemented and works on KDE and wlroots compositors.
- The SayWrite IBus bridge is the primary GNOME Wayland insertion path with engine swap/restore and retry logic.
- In-app host installation builds the release binary if needed and runs the install script.
- Desktop detection (GNOME Wayland, Other Wayland, X11, Other) with per-profile dependency checks and package hints.
- Host-side unit tests cover IBus parsing, insertion backend classification, result-kind mapping, error sanitization, and toggle debounce.

## Design Principles

1. Simple and clean beats clever and fragile.
2. Setup lives inside the app, not in a README.
3. Cleanup is the product differentiator.
4. Local-first, offline-capable after model download.
5. Auto-detect acceleration.
6. Flatpak-first; host integration is a companion, not a hack.
7. Make changes with the user base in mind. Don't just get it working on the developer's PC. A change that works in the dev checkout but relies on paths, state, or binaries that a fresh Flatpak user doesn't have is not a fix — it's a regression waiting to be reported. Before marking work done, ask: "would this still work on a machine that has only ever installed the `.flatpak` bundle?" If the answer is no or "not sure", fix that gap before shipping.

## Repo Hygiene

- Keep `README.md` and `AGENTS.md` up to date whenever product behavior, supported workflows, setup steps, or architecture assumptions change.
- Remove dead compatibility shims, outdated scripts, and stale copy when they no longer reflect the supported Rust app + `saywrite-host` architecture.
- Treat clearly marked historical docs in `docs/` as archival context, not as current source of truth.

## Testing

- Run `cargo test`.
- Run `cargo check` before deeper changes.
- Prefer checking both binaries when touching shared interfaces or host behavior.
- No mocking of filesystem or GStreamer when real objects are practical.
- The user runs SayWrite via the **installed Flatpak** (`io.github.fabio.SayWrite`), not the debug binary. After any source code change the user needs to test, rebuild and reinstall the Flatpak before asking them to try anything. Do not ask the user to test until the reinstall is complete.
  ```
  flatpak-builder --user --install --force-clean build-dir flatpak/io.github.fabio.SayWrite.json
  ```
- **Always verify the developer is running the latest Flatpak build.** There have been instances where launching SayWrite from GNOME loaded a stale version that did not reflect recent changes. If the user reports behavior that contradicts the code, ask them to rebuild and relaunch before investigating further.

## Docs

- `docs/README.md` — which docs are current vs archived
- `docs/next_steps.md` — active plan and release priorities
- `docs/holistic_review.md` — current codebase assessment
- `docs/architecture.md` — historical design rationale and boundaries
- `docs/roadmap.md` — high-level product stages
- `docs/implementation_plan.md` — archived earlier planning phase
- `docs/ship_todo.md` — archived earlier ship checklist
