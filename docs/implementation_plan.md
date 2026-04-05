# Archived Implementation Plan

This document is historical and no longer reflects the current repo state.

Use these instead:

- `next_steps.md` for the active plan
- `holistic_review.md` for the current codebase assessment

This file is intentionally kept only as a record of an earlier planning phase.
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
