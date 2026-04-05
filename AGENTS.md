# AGENTS.md — SayWrite Codebase Guide

SayWrite is a Linux-first dictation app with a Rust GTK4/libadwaita shell and a Rust host daemon. The goal is simple: speak, get cleaned polished text committed to any text field, with no external setup docs.

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
| `src/app.rs` | GTK application + activation flow |
| `src/lib.rs` | shared library surface for app and host |
| `src/config.rs` | `AppSettings`, `ProviderMode`, `ModelSize`, JSON load/save, XDG paths |
| `src/cleanup.rs` | `cleanup_transcript()` |
| `src/dictation.rs` | microphone session control, whisper.cpp CLI transcription, cloud handoff, session state |
| `src/model_installer.rs` | model download, validation, and cache flow |
| `src/host_integration.rs` | D-Bus/Unix socket text insertion client, clipboard fallback |
| `src/host_api.rs` | shared D-Bus constants and host status types |
| `src/runtime.rs` | readiness probing |
| `src/ui/main_window.rs` | main dictation window |
| `src/ui/onboarding.rs` | onboarding carousel |
| `src/ui/preferences.rs` | preferences and diagnostics |
| `src/bin/saywrite-host/` | host daemon, hotkey, insertion, and D-Bus service code |
| `scripts/install-host.sh` | installs `saywrite-host` as a user service + D-Bus service |
| `scripts/install-gnome-shortcut.sh` | GNOME custom shortcut fallback helper |

## Key Facts

- `AppSettings.provider_mode` is an enum: `Local` or `Cloud`.
- `AppSettings.local_model_path` is optional and stored as a `PathBuf`.
- `AppSettings.model_size` is an enum: `Tiny`, `Base`, or `Small`.
- `AppSettings.global_shortcut_label` defaults to `Super+Alt+D`.
- `cleanup_transcript()` is deterministic and should stay conservative.
- `saywrite-host` exists and answers D-Bus calls on `io.github.saywrite.Host`.
- The app prefers D-Bus for host insertion and falls back to the legacy Unix socket/clipboard path.
- The host now attempts XDG GlobalShortcuts portal registration at startup and reports status over D-Bus.
- Host insertion currently prefers command backends such as `wtype`/`xdotool`, then `ibus`, then clipboard tools.
- IBus is present only as a command-line backend right now, not a first-class input-context integration.

## Current State

- GTK UI is polished enough for onboarding and diagnostics.
- Microphone capture and cleanup work.
- Local transcription works if `whisper-cli` and a model are installed.
- Cloud mode is wired through the configured OpenAI-compatible API base and key.
- `saywrite-host` starts, owns the D-Bus name, and handles `GetStatus`, `InsertText`, and `ToggleDictation`.
- The host emits D-Bus signals for dictation state, ready text, and insertion results.
- Global shortcut support is in progress: portal registration is implemented, but desktop support and fallback behavior still matter.
- Native IBus text commit is still pending; current insertion remains backend/fallback-driven.
- There is now a repo-local host install script for running the companion outside Flatpak.

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
- Prefer checking both binaries when touching shared interfaces or host behavior.
- No mocking of filesystem or GStreamer when real objects are practical.

## Docs

- `docs/architecture.md` — component design and D-Bus boundaries
- `docs/roadmap.md` — V0 → V4 milestones
- `docs/implementation_plan.md` — implementation plan and host daemon scope
- `docs/code_review.md` — current review findings and follow-up notes
- `docs/security_review.md` — security risks and mitigations
- `docs/ui_review.md` — UI review notes
- `docs/ship_todo.md` — pre-ship checklist and gaps
