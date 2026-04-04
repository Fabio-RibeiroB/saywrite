# AGENTS.md — SayWrite Codebase Guide

SayWrite is a Linux-first dictation app (Flatpak) inspired by Wispr Flow. The goal: speak, get cleaned polished text committed to any text field. Simple, no external setup docs required.

## Stack

- **Language**: Python 3.11+
- **UI**: GTK4 + libadwaita (PyGObject `gi`)
- **Audio**: GStreamer 1.0
- **ASR**: whisper.cpp (local), configurable cloud API (OpenAI-compatible)
- **Packaging**: Flatpak (`io.github.fabio.SayWrite`, GNOME 46 runtime)

## Module Map

| File | Role |
|------|------|
| `saywrite/app.py` | GTK application + all UI code (`SayWriteApplication`, `SayWriteWindow`) |
| `saywrite/config.py` | `AppSettings` dataclass, load/save JSON to `~/.config/saywrite/settings.json` |
| `saywrite/audio.py` | `MicrophoneMonitor` — GStreamer pipeline, emits level % and state strings |
| `saywrite/cleanup.py` | `cleanup_transcript()` — filler removal, spoken punctuation → symbols, capitalization |
| `saywrite/backend.py` | `WhisperCppBackend`, `BackendProbe`, `probe_backends()` — invokes whisper.cpp CLI |
| `saywrite/hardware.py` | GPU vendor detection, acceleration path selection (CUDA/Vulkan/CPU), tool discovery |
| `saywrite/transcription.py` | `run_local_transcription()`, `transcribe_recorded_microphone()` — high-level wrappers |
| `saywrite/recorder.py` | `record_microphone_clip()` — GStreamer subprocess record to WAV |
| `saywrite/model_installer.py` | `install_default_model()` — downloads ggml-base.en.bin via whisper.cpp script |
| `saywrite/mock_dictation.py` | `MockDictationController` — deterministic fake ASR sequences for UX dev |
| `saywrite/providers.py` | Model metadata: names, summaries, pill labels, provider copy strings |
| `saywrite/paths.py` | XDG data dirs: `app_data_dir()`, `local_models_dir()` |
| `saywrite/theme.css` | GTK4 CSS — warm earth-tone palette, card styles, button shapes |
| `saywrite/main.py` | Entry point shim |

## Key Concepts

### Settings (`config.py`)
`AppSettings` fields: `provider_mode` ("local"/"cloud"), `onboarding_complete`, `local_model_path`, `cloud_api_base`, `cloud_api_key`. Persisted as JSON. Load with `load_settings()`, save with `save_settings(settings)`.

### Audio pipeline (`audio.py`)
`MicrophoneMonitor(on_level_cb, on_state_cb)` — call `.start()` / `.stop()`. Level callback receives `int` 0–100. State callback receives human-readable string. Uses GStreamer `autoaudiosrc → level → fakesink`.

### Cleanup pipeline (`cleanup.py`)
`cleanup_transcript(text: str) -> str` — deterministic rule-based. Removes: um/uh/er/ah/like. Converts: "question mark" → "?", "open bracket" → "(", etc. Then capitalizes and cleans whitespace.

### Backend (`backend.py`, `hardware.py`)
`detect_local_runtime()` → `LocalRuntime(gpu_vendor, acceleration, whisper_cli_path, runnable, cmake_available, vulkan_available)`.
`probe_backends(model_path, api_key, mode)` → `BackendProbe(local_runtime, local_model_configured, cloud_configured)`.

### Mock dictation (`mock_dictation.py`)
`MockDictationController(mode)` — `advance()` returns next chunk string or `None` at end. `reset()` restarts. `current_chunk()` returns last chunk. Used for UI development before real ASR is wired.

## Architecture Intent

Three future D-Bus services (see `docs/architecture.md`):
1. **`io.github.fabio.SayWrite.App`** — GTK UI (this Flatpak)
2. **`io.github.fabio.SayWrite.Service`** — speech pipeline worker
3. **`io.github.fabio.SayWrite.IBus`** — host IBus engine for text injection

Currently only #1 exists. The app is at V0 (scaffold). V1 milestone = real press-to-dictate with cleaned text in preview.

## Current State (V0)

- All UI in one scrollable `SayWriteWindow` (1180×820) — a developer demo page, not a product UI
- Microphone capture works (GStreamer level monitor)
- Cleanup pipeline works
- Backend/hardware detection works
- Mock dictation works
- Real transcription works if whisper.cpp + model are installed externally
- No global hotkey, no IBus, no system-wide text insertion yet

## Design Principles

1. Simple and clean beats clever and fragile
2. Setup lives inside the app, not in a README
3. Cleanup (raw → polished text) is the product differentiator
4. Local-first, offline-capable after model download
5. Auto-detect acceleration — never ask users to choose CUDA vs Vulkan
6. Flatpak-first; host integration is a companion, not a hack

## Testing

Tests in `tests/`. Run with `pytest`. Each module has a corresponding `test_*.py`. No mocking of filesystem or GStreamer — tests use real objects where possible.

## Docs

- `docs/architecture.md` — component design, D-Bus boundaries, IBus rationale
- `docs/roadmap.md` — V0 → V4 milestones
