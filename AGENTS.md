# AGENTS.md — SayWrite Codebase Guide

SayWrite is a Linux-first dictation app with a Rust GTK4/libadwaita shell and a Rust host daemon. The goal is simple: speak, get cleaned polished text committed to any text field, with no external setup docs.

## Stack

- **Language**: Rust 2021
- **UI**: GTK4 + libadwaita
- **Audio**: GStreamer 1.0
- **ASR**: whisper.cpp (local), configurable cloud API (OpenAI-compatible)
- **Packaging**: Flatpak (`io.github.fabio.SayWrite`, GNOME 46 runtime)

## Current Layout

| File | Role |
|------|------|
| `src/main.rs` | GTK application entry point |
| `src/app.rs` | GTK application + activation flow |
| `src/config.rs` | `AppSettings`, `ProviderMode`, JSON load/save, XDG paths |
| `src/cleanup.rs` | `cleanup_transcript()` |
| `src/dictation.rs` | microphone session control, whisper.cpp CLI transcription, session state |
| `src/model_installer.rs` | model download and cache flow |
| `src/host_integration.rs` | D-Bus/Unix socket text insertion client, clipboard fallback |
| `src/host_api.rs` | shared D-Bus constants and host status types |
| `src/runtime.rs` | readiness probing |
| `src/ui/main_window.rs` | main dictation window |
| `src/ui/onboarding.rs` | onboarding carousel |
| `src/ui/preferences.rs` | preferences and diagnostics |
| `src/bin/saywrite-host/` | host daemon, hotkey, insertion, and D-Bus service code |

## Key Facts

- `AppSettings.provider_mode` is an enum: `Local` or `Cloud`.
- `AppSettings.local_model_path` is optional and stored as a `PathBuf`.
- `cleanup_transcript()` is deterministic and should stay conservative.
- `saywrite-host` exists and answers D-Bus calls on `io.github.saywrite.Host`.
- The app prefers D-Bus for host insertion and falls back to the legacy Unix socket/clipboard path.
- Hotkey registration is still fallback-oriented, not portal-native yet.
- IBus insertion is still pending.

## Current State

- GTK UI is polished enough for onboarding and diagnostics.
- Microphone capture and cleanup work.
- Local transcription works if `whisper-cli` and a model are installed.
- `saywrite-host` starts, owns the D-Bus name, and responds to `GetStatus`.
- No global hotkey registration yet.
- No IBus text commit yet.

## Design Principles

1. Simple and clean beats clever and fragile.
2. Setup lives inside the app, not in a README.
3. Cleanup is the product differentiator.
4. Local-first, offline-capable after model download.
5. Auto-detect acceleration.
6. Flatpak-first; host integration is a companion, not a hack.

## Testing

- Run `cargo test`.
- Run `cargo check` before deeper changes.
- No mocking of filesystem or GStreamer when real objects are practical.

## Docs

- `docs/architecture.md` — component design and D-Bus boundaries
- `docs/roadmap.md` — V0 → V4 milestones
- `docs/implementation_plan.md` — implementation plan and host daemon scope
