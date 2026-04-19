# Holistic Review

This review summarizes the current structural issues in the SayWrite codebase and the current state of live dictation into text fields.

## Executive Summary

The codebase is in reasonable shape for:

- microphone capture
- transcription (local whisper.cpp and cloud OpenAI-compatible API)
- cleanup
- clipboard delivery
- direct insertion on supported host setups (GNOME Wayland via IBus bridge)
- in-app host installation with progress feedback
- global shortcut capture with GNOME keybinding suspend/restore
- host daemon lifecycle tied to GUI (starts on launch, stops and is masked on close)
- explicit insertion capability and result-kind reporting
- host-side toggle debounce and error sanitization

It is not yet in a universal state for:

- direct insertion into arbitrary focused text fields across all Linux desktops and environments

The main gap is not transcription quality. The remaining gap is that insertion is still host- and desktop-dependent, while some product copy and support expectations can still become broader than the implementation can guarantee.

## What Is Working

- `dictation.rs` handles recording and transcription end to end, with silence detection to filter out empty recordings.
- `cleanup.rs` has focused tests and produces deterministic cleaned text.
- The app/host split is directionally correct: GTK app in Flatpak, host daemon outside sandbox.
- The D-Bus host path is the primary control plane and is easier to reason about than a socket-only design.
- Clipboard fallback is broadly practical.
- The host includes an IBus bridge for GNOME Wayland with engine swap/restore logic and retry on failure.
- `saywrite-host` refuses to start unless the app owns its D-Bus name (`io.github.fabio.SayWrite`), preventing orphan daemons.
- Direct insertion is working on supported setups, including the currently reported GNOME Wayland environment.
- `wtype` (Wayland) and `xdotool` (X11) insertion paths exist and are probed, but remain untested on real hardware.
- Insertion capability and result kinds are explicit: `typing`/`clipboard-only`/`notification-only`/`unavailable` and `typed`/`copied`/`notified`/`failed`.
- `host_setup.rs` provides desktop detection (GNOME Wayland, Other Wayland, X11, Other) with per-profile dependency checks and package hints.
- In-app host installation via `install_host_companion()` builds the release binary and runs the install script.
- Shortcut capture dialog suspends GNOME keybindings during capture and restores them afterward.
- Shortcut changes from within Flatpak update GNOME keybindings directly via `flatpak-spawn --host gsettings`.
- Host-side toggle debounce (900ms) prevents repeated shortcut activations from wedging the daemon.
- Error sanitization maps typed `DictationError` variants to user-friendly messages.
- `cargo check` passes on the current tree.

## Current State

The repo has moved substantially since earlier reviews:

- `saywrite-host` includes a dedicated `input.rs` module with IBus bridge and GlobalShortcuts portal support.
- The insertion layer (`insertion.rs`) prefers `IbusEngine` before `wtype` on GNOME Wayland, then `xdotool` on X11, then clipboard tools, then notification.
- The host daemon checks `app_session_is_active()` before starting, so it won't run without the GUI.
- The app masks the host service on shutdown, preventing orphan activation after quit.
- `host_setup.rs` handles desktop-aware diagnostics with per-profile dependency requirements and Ubuntu/Zorin package hints.
- Settings sync from Flatpak to host via `flatpak-spawn --host` on save.
- The code falls back to clipboard and notification delivery when true typing is unavailable.

That means older assessments that treated Wayland insertion as only `wtype`-based, or that said direct insertion was not working at all, are now stale.

The broader product conclusion still does not become "works everywhere", because:

- the diagnostics and UI now distinguish capability much better, but support still depends on the active backend and desktop
- the codebase still allows fallback delivery to count as a successful result category
- desktop and host-tool dependencies still determine whether real typing is available

## Current Code Smells

### 1. UI orchestration is still too stateful

`src/ui/main_window/` has improved, but it still owns a large amount of application flow:

- dictation start/stop state
- host signal consumption
- transcript presentation
- clipboard fallback
- keyboard shortcuts

This is manageable for now, but the UI layer still knows too much about host behavior and insertion outcomes.

### 2. Async integration is polling-heavy, though cleaner than before

GTK-facing async work is still bridged through timer polling. A shared helper exists in `src/ui/async_poll.rs`, which is an improvement, but the architecture still depends on:

- worker thread
- channel
- periodic GTK poll

This is not broken, but it is still a code smell because state transitions remain spread across multiple timers instead of a single event model.

### 3. Host D-Bus endpoints own business logic

`src/bin/saywrite-host/dbus.rs` is doing transport, dictation control, insertion, state tracking, and signal emission. That makes it harder to:

- test behavior in isolation
- change insertion policy safely
- distinguish backend errors from orchestration errors

### 4. Error handling is partly typed, but UI still interprets by substring

Host errors are now sanitized through `sanitize_error()` which maps `DictationError` variants to user-friendly messages. However, the UI layer still interprets some failures by matching message substrings. That creates drift risk between backend behavior and frontend messaging.

### 5. Runtime capability reporting is explicit, but product messaging can still overgeneralize

The repo has explicit insertion capability and result categories (`host_api.rs`), runtime probing (`runtime.rs`), and desktop-aware diagnostics (`host_setup.rs`). The remaining issue is that product-level messaging can still overgeneralize from the active setup.

The actual backend may be:

- real typing through IBus, `wtype`, or `xdotool`
- clipboard-only fallback
- notification-only fallback

This is no longer a pure code-structure gap. It is now mostly a product-messaging and support-matrix gap.

### 6. External capability detection is fragile

Hotkey and insertion support still depend on parsing host command behavior and command presence:

- `busctl`
- `ibus`
- `gdbus`
- `wtype`
- `xdotool`
- `wpctl`
- `pactl`
- clipboard tools

This is practical, but not robust enough to support strong claims like "works in any text field".

### 7. Test coverage is improving, but still narrow overall

There is now real host-side unit coverage, including:

- backend classification and result-kind mapping (`insertion.rs`)
- IBus parsing (`input.rs`)
- error sanitization (`service.rs`)
- toggle debounce (`service.rs`)

The remaining thin areas are:

- host D-Bus behavior
- settings migration and runtime probing
- end-to-end host-side dictation state transitions
- IBus engine restore and commit lifecycle under failure
- `host_setup.rs` desktop detection logic

## Why The Product Still Cannot Promise Universal Text-Field Dictation

Direct insertion is now working on supported setups. The remaining issue is not "it does not work", but "it is not universally guaranteed".

### 1. "Insertion" still includes non-typing fallbacks

In `src/bin/saywrite-host/insertion.rs`, these are real typing-style backends:

- `IbusEngine`
- `wtype`
- `xdotool`

The following are not text-field insertion:

- `wl-copy`
- `xclip`
- `xsel`
- `notify-send`

Those paths can still return success-like messages, which means the system can report successful delivery even when it only copied text or showed a notification. The result-kind system (`typed` vs `copied` vs `notified` vs `failed`) distinguishes these, but the UI still needs to surface the distinction clearly to users.

### 2. Capability reporting is no longer blind, but support still depends on the active backend

The host now reports insertion capability and backend explicitly. That is a real fix. The remaining issue is operational rather than purely representational:

- clipboard-only environments are now represented more honestly
- direct typing still depends on the backend the machine actually has
- the support claim can still be broader than the validated matrix

### 3. Focus handoff is still not explicit

For manual "type into app" behavior, the API shape includes `delay_seconds`, but the D-Bus path ignores it. The current system can still work when the host-side path commits text through the active input context, but focus behavior is still implicit rather than modeled explicitly.

### 4. The product promise still exceeds Linux desktop reality

The current architecture depends on desktop-specific host-side input paths. That can work well in supported setups, but not universally.

## Wayland Reality

This repo no longer relies only on `wtype` on Wayland. It now uses an IBus bridge on GNOME Wayland and falls back to `wtype` elsewhere.

That is a meaningful improvement, and it appears to be working on at least some current setups. It still does not justify a universal promise. Direct insertion on Wayland still depends on:

- GNOME Wayland and IBus behavior being compatible with the active application
- compositor support
- host environment support
- the host tools and services being installed and available outside the Flatpak

That is not a universal Linux capability. A generic Flatpak install cannot promise direct typing into arbitrary host apps on Wayland.

## X11 Reality

X11 is more permissive, and `xdotool` can work in many cases. But it is still not universal because it depends on:

- the user actually running X11
- the tool being installed
- focus being on the intended target field
- the target app accepting synthetic key events

So X11 is more viable than Wayland, but still not a guaranteed "any text field" story for all users.

## Flatpak Constraint

The Flatpak itself cannot directly inject text into arbitrary host windows. The separate `saywrite-host` daemon is the right architectural response to that restriction, but the daemon still depends on host-specific tools, desktop behavior, and the IBus bridge on Wayland.

This means the current product is best understood as:

- reliable transcription and cleanup
- reliable clipboard delivery
- direct typing on supported host setups

## What Has Been Addressed

### 1. Insertion capability is now honest ✅

Explicit capability states exist in `host_api.rs`:

- `typing`
- `clipboard-only`
- `notification-only`
- `unavailable`

Result kinds also distinguish outcomes:

- `typed`
- `copied`
- `notified`
- `failed`

### 2. "Typed" is separated from "delivered" ✅

Host responses carry explicit `result_kind` fields. The D-Bus `InsertionResult` signal includes `(ok, result_kind, message)`.

### 3. Backend reporting is explicit in diagnostics ✅

The runtime probe (`runtime.rs`) reports insertion capability and backend. The host setup diagnostics (`host_setup.rs`) show desktop profile, missing dependencies, and package hints.

### 4. Product language has been narrowed ✅

The README and onboarding now use "Clipboard Mode" and "Direct Typing Mode" as user-facing terms, with honest claims about supported environments.

## What Should Still Change

### 1. Add targeted tests for insertion policy and host flow

The next tests should focus on:

- host D-Bus state transitions
- IBus engine restore behavior under failure
- `host_setup.rs` desktop detection logic
- settings migration and runtime probing

### 2. Tighten UI error interpretation

Move the UI away from substring matching on error messages toward typed error handling that uses the sanitized messages from `service.rs`.

### 3. Consolidate async state model

Consider moving from timer-based polling to a single event-driven state machine for dictation lifecycle, so that the UI reacts to host signals rather than polling for state changes.

## Recommended Product Language

Today the honest promise is closer to:

- "Dictate, clean, and copy anywhere."
- "Direct typing works on supported host setups, including the current IBus-based GNOME Wayland path."

It is still not accurate to promise:

- "Live dictation into any text field on Linux."

## Bottom Line

SayWrite is already useful because transcription and clipboard flow work well, and direct insertion is now working on supported setups. The IBus bridge, explicit capability reporting, in-app host installation, and daemon lifecycle management are all meaningful improvements. But the current host insertion layer still does not justify the stronger UX promise of universal direct text-field insertion across Linux.

The main issue is not a single bug. It is a mismatch between:

- what the insertion layer can really do on the current host setup
- what the diagnostics report
- what the product currently implies

The next meaningful improvement is still not another transcription change. It is:

- keeping insertion capability explicit and truthful
- separating real typing success from clipboard or notification delivery
- documenting and verifying the supported insertion environments
- expanding cross-desktop validation (KDE Plasma, wlroots, X11)
