# SayWrite Next Steps

SayWrite has crossed the main technical hurdle: hotkey-driven dictation, cleanup, and direct insertion now work on a real GNOME Wayland machine through the host daemon and IBus bridge.

The next phase is not new architecture. It is release readiness.

## Current State

- The GTK app exists and acts as the setup and diagnostics surface.
- `saywrite-host` owns the real dictation workflow.
- Global hotkey dictation works through the host path.
- Local transcription works end to end.
- Cloud transcription exists.
- Direct insertion works on the currently validated GNOME Wayland setup.
- Clipboard and notification fallbacks exist for degraded environments.

## Release Priorities

## 1. Package the Host Companion Cleanly

The Flatpak app is only half of the product. The host companion now needs a clean install story for normal users.

- keep the Flatpak as the main UI distribution
- package `saywrite-host` as a small native companion
- keep `scripts/install-host.sh` as a fallback, not the primary story
- make the app explain the host requirement in product language

Success bar:

- a user can install the app, install the host companion, and start dictating without reading source-oriented setup notes

## 2. Harden the IBus Path

The IBus bridge is now the critical GNOME Wayland path and needs reliability work, not reinvention.

- test repeated dictation in the same field
- test focus changes during dictation and commit
- verify the previous engine is always restored
- handle empty transcript and cancel paths cleanly
- improve failure cleanup and logging around engine swap

Success bar:

- repeated dictation stays reliable without restarting the daemon

## 3. Validate the Desktop Support Matrix

One working GNOME Wayland machine is a breakthrough, not a full release matrix.

- test another GNOME Wayland machine or a fresh user account
- test X11 with `xdotool`
- test a wlroots compositor where `wtype` should work
- verify clipboard and notification fallback behavior when direct typing is unavailable
- test several app types: browser, GTK, Qt, Electron, terminal/chat tools

Success bar:

- SayWrite has an explicit supported matrix instead of a broad implied promise

## 4. Keep Capability Reporting Honest

The product must clearly distinguish between:

- direct typing
- clipboard fallback
- notification fallback
- unavailable

This work is already meaningfully implemented in the host, runtime, and UI layers, but it still needs to be finished consistently across the whole product.

- keep diagnostics explicit about the active backend
- keep onboarding honest about what the current machine supports
- avoid claiming “any text field” unless a real typing backend is active
- keep result messaging distinct for typed, copied, and notification outcomes

Success bar:

- users can tell exactly what SayWrite will do on their machine before they start dictating

## 5. Add Host-Focused Regression Tests

The risky part of the product is now host behavior, not transcript cleanup.

- keep expanding the new host-side unit coverage
- add tests around IBus engine restore behavior
- add tests for host D-Bus state transitions
- add tests for fallback result reporting, not just backend classification
- keep `cargo test` and `cargo clippy --all-targets --all-features -- -D warnings` clean

Success bar:

- refactors in the host daemon do not silently break insertion behavior

## 6. Polish the Hotkey-First UX

The app should feel like a control panel for a hotkey-first product, not the center of the workflow.

- improve first-run guidance around the shortcut and host companion
- end onboarding with a real dictation test
- improve diagnostics copy for direct typing vs fallback modes
- keep the app focused on setup, trust, and recovery

Success bar:

- the core user journey is: install, press shortcut, speak, see text land where expected

## Recommended Order

1. Package the host companion cleanly.
2. Harden the IBus path and engine restore behavior.
3. Validate the support matrix across GNOME Wayland, X11, and another Wayland compositor.
4. Finish honest diagnostics and onboarding copy.
5. Add host-focused regression tests.

## Release Goal

SayWrite is ready for a broader first public release when these are true:

- hotkey dictation works without opening the app
- direct insertion works reliably on at least one supported GNOME Wayland setup
- X11 support is validated on a real machine
- degraded modes are honest and understandable
- host installation is clear from inside the app
- build and lint checks stay clean

## Non-Blocking After Beta

These matter, but they should not block a first supported release:

- tray icon and quick controls
- more aggressive cleanup customization
- application-aware formatting profiles
- wider compositor coverage beyond the first supported matrix
