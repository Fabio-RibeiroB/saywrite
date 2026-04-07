# SayWrite Next Steps

SayWrite has crossed the main technical hurdle: hotkey-driven dictation, cleanup, and direct insertion now work on a real GNOME Wayland machine through the host daemon and IBus bridge.

The next phase is not new architecture. It is release readiness and polish.

## Current State

- The GTK app exists and acts as the setup and diagnostics surface.
- `saywrite-host` owns the real dictation workflow.
- Global hotkey dictation works through the host path.
- Local transcription works end to end.
- Cloud transcription exists.
- Direct insertion works on the currently validated GNOME Wayland setup.
- Clipboard and notification fallbacks exist for degraded environments.
- Host daemon lifecycle is now tied to the GUI (starts on app launch, stops on close).
- Mic capture hardened: PipeWire source detection, RMS silence rejection.
- IBus path hardened: race condition fixed, retry logic, comprehensive logging.
- Setup/install concerns are now split out of transport code (`host_setup.rs` vs `host_integration.rs`).
- Host D-Bus transport is now thinner: daemon workflow/state lives in `service.rs` instead of `dbus.rs`.

## Product Framing

The product should be presented as two user-facing modes, even if the implementation still uses multiple processes under the hood:

- `Clipboard Mode`:
  - works with the Flatpak app alone
  - records, transcribes, cleans, and copies text
  - is the safe default when direct typing is not enabled
- `Direct Typing Mode`:
  - enables host integration outside the sandbox
  - allows hotkey-driven dictation and text insertion into supported host apps
  - should feel like a single guided setup step, not "install a separate program manually"

The key UX goal is that users experience one product, not "app plus helper". If the host-side integration remains visible and manual, the product will feel meaningfully worse than apps that simply stay open and minimized.

## Release Priorities

## 1. Package the Host Companion Cleanly ✅

The Flatpak app is only half of the full direct-typing story. The host companion now needs a clean install story for normal users, ideally behind a single "Enable Direct Typing" flow inside the app.

**Status: DONE**
- In-app installation via `host_setup::install_host_companion()` works end-to-end
- Settings → Diagnostics shows "Enable Direct Typing" button when source repo is available
- Fallback instructions shown for packaged/Flatpak-only users
- Host daemon lifecycle now tied to GUI: starts on app launch, stops on app close
- `scripts/install-host.sh` is fallback, not primary story

## 2. Harden the IBus Path ✅

The IBus bridge is now the critical GNOME Wayland path and needs reliability work, not reinvention.

**Status: DONE**
- Fixed race condition in `clear_pending_commit` by making it async
- Added retry logic with 250ms delay on engine restore failure
- Comprehensive logging at engine swap, commit, timeout, and failure points
- Empty text guard prevents full round-trip for no-op calls
- Repeated dictation tested and reliable without daemon restart

## 3. Validate the Desktop Support Matrix

One working GNOME Wayland machine is a breakthrough, not a full release matrix.

**Status: SCOPED, NOT STARTED (~75% manual testing)**
- Requires testing on: another GNOME Wayland machine, X11 with `xdotool`, wlroots compositor with `wtype`
- Fallback testing: verify clipboard and notification behavior when direct typing unavailable
- App type testing: browser, GTK, Qt, Electron, terminal/chat
- Estimated effort: ~20-30 manual test cycles on different machines

Success bar:

- SayWrite has an explicit supported matrix instead of a broad implied promise

## 4. Keep Capability Reporting Honest

The product must clearly distinguish between:

- direct typing
- clipboard fallback
- notification fallback
- unavailable

**Status: PARTIALLY DONE**
- Runtime probing complete: GPU detection, whisper.cpp discovery, local model check
- Diagnostics page shows insertion backend and hotkey status
- Result messages distinguish typed/copied/notification outcomes
- **TODO**: Onboarding should present mode choice (Clipboard vs Direct Typing) more explicitly, informed by actual host capability
- **TODO**: Avoid claiming direct typing in onboarding if `host_status()` is unavailable

Success bar:

- users can tell exactly what SayWrite will do on their machine before they start dictating

## 5. Add Host-Focused Regression Tests

The risky part of the product is now host behavior, not transcript cleanup.

**Status: PARTIALLY DONE**
- Host regression tests added (commit `41db094`)
- Current coverage includes backend classification, result-kind mapping, IBus parsing
- Recent host refactors separated transport/setup logic from daemon workflow, which reduces drift risk but increases the need for explicit state-machine tests
- **TODO**: Expand IBus engine restore behavior tests
- **TODO**: Add tests for host service state transitions and D-Bus lifecycle
- **TODO**: Test fallback result reporting (not just backend classification)

Success bar:

- refactors in the host daemon do not silently break insertion behavior

## 6. Keep Structural Boundaries Clean

Recent cleanup improved the code shape, but a few large boundaries still own too much behavior.

**Status: IN PROGRESS**
- Completed:
  - Moved host install/setup workflow out of `host_integration.rs` into `host_setup.rs`
  - Moved host daemon workflow/state machine out of `src/bin/saywrite-host/dbus.rs` into `src/bin/saywrite-host/service.rs`
- **TODO**: Split `src/ui/main_window.rs` so widget construction, host event handling, and dictation state transitions are not all in one file
- **TODO**: Revisit `src/app.rs` service lifecycle management (`systemctl --user start/stop/mask/unmask`) and make the GUI-to-host boundary less blunt
- **TODO**: Decide whether the Unix socket fallback still earns its maintenance cost now that D-Bus is the primary control plane

Success bar:

- transport layers stay thin, feature work lands in service modules, and UI files stop being orchestration bottlenecks

## 7. Polish the Hotkey-First UX

The app should feel like a control panel for a hotkey-first product, not the center of the workflow.

**Status: PARTIALLY DONE**
- **Completed**:
  - Waveform animation during listening state
  - Mic device picker in Settings
  - Audio pause toggle (mute PC audio during dictation)
  - Inline settings page (stack-based, no popup window)
  - GPU detection in Diagnostics
- **TODO**: First-run mode choice between Clipboard Mode and Direct Typing Mode
- **TODO**: Onboarding should inform choice based on actual host capability
- **TODO**: End onboarding with dictation test
- **TODO**: Improve diagnostics copy for direct typing vs fallback

Success bar:

- the core user journey is: install, press shortcut, speak, see text land where expected

## 8. Integrate The `ui-improvements` Worktree Carefully

There is a separate UI worktree with useful UX polish that should be merged into the current branch without replacing the current app logic. The goal is to preserve the existing working behavior and only bring over the UX/UI improvements that still fit the current architecture.

**Status: SCOPED**
- Merge target: preserve current app/runtime/host behavior and only integrate the presentation and UX improvements
- Source: `worktree-ui-improvements` / `ui-improvements` worktree
- Review standard: every imported UI change should be checked against the current branch so stale assumptions do not overwrite newer host/setup/runtime logic

Planned integration scope:
- `src/ui/main_window.rs`
  - Replace the simple listening spinner with animated waveform bars
  - Add `GtkRevealer` crossfade transitions for activity, setup panel, transcript, and action row
  - Add inline setup resolution actions:
    - local model download from the main window
    - inline cloud API key entry
    - settings fallback action
  - Add a header insertion-mode chip showing Direct Typing / Clipboard / Notification / Offline
  - Rework transcript display to an editable `TextView` with live word/character counts and a Retry action
- `src/ui/onboarding.rs`
  - Add mic test recording feedback with a pulsing progress indicator and status text
  - Add cancellable model download flow with a visible Cancel button and rough ETA text
- `src/ui/preferences.rs`
  - Add a brief "Settings saved" toast after debounced saves
- `resources/style.css`
  - Add styles for waveform bars, insertion chip, editable transcript view, and toast
- `src/model_installer.rs`
  - Add cancellable model download helper used by onboarding

Success bar:

- the UI gains the polish from the worktree without regressing current host behavior, setup logic, or diagnostics accuracy

## Recommended Order for Remaining Work

1. ✅ Package the host companion cleanly (DONE)
2. ✅ Harden the IBus path (DONE)
3. → **Validate the desktop support matrix** (next: manual testing on other machines)
4. → **Improve onboarding honest reporting** (inform mode choice by actual host capability)
5. → **Expand host regression tests** (service/D-Bus lifecycle, fallback reporting)
6. → **Keep structural boundaries clean** (split `main_window.rs`, tighten app/service lifecycle)
7. → **Integrate the `ui-improvements` worktree carefully** (bring over UX polish without clobbering current logic)

## Release Goal

SayWrite is ready for a broader first public release when these are true:

- ✅ hotkey dictation works without opening the app (when host daemon is running)
- ✅ direct insertion works reliably on at least one supported GNOME Wayland setup
- ⏳ X11 support is validated on a real machine (TODO: priority #3)
- ⏳ degraded modes are honest and understandable (TODO: priority #4 onboarding work)
- ✅ host installation is clear from inside the app
- ✅ build and lint checks stay clean (`cargo clippy --all-targets --all-features -- -D warnings`)

## Non-Blocking After Beta

These matter, but they should not block a first supported release:

- tray icon and quick controls
- more aggressive cleanup customization
- application-aware formatting profiles
- wider compositor coverage beyond the first supported matrix
