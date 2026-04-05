# Holistic Review

This review summarizes the current structural issues in the SayWrite codebase and the current state of live dictation into text fields.

## Executive Summary

The codebase is in reasonable shape for:

- microphone capture
- transcription
- cleanup
- clipboard delivery
- direct insertion on supported host setups

It is not yet in a universal state for:

- direct insertion into arbitrary focused text fields across all Linux desktops and environments

The main gap is not transcription quality. The remaining gap is that insertion is still host- and desktop-dependent, while some product copy and support expectations can still become broader than the implementation can guarantee.

## What Is Working

- `dictation.rs` handles recording and transcription end to end.
- `cleanup.rs` has focused tests and produces deterministic cleaned text.
- The app/host split is directionally correct: GTK app in Flatpak, host daemon outside sandbox.
- The D-Bus host path is the primary control plane and is easier to reason about than a socket-only design.
- Clipboard fallback is broadly practical.
- The host now includes an IBus bridge for GNOME Wayland.
- The current tree compiles again, including the host-side IBus path.
- Direct insertion is now working on supported setups, including the currently reported environment.

## Current State

The repo has moved substantially since earlier reviews:

- `saywrite-host` includes a dedicated `ibus.rs` bridge and prefers it on GNOME Wayland.
- The insertion layer prefers `IbusEngine` before `wtype` on GNOME Wayland.
- The code still falls back to clipboard and notification delivery when true typing is unavailable.
- `cargo check` passes on the current tree.

That means older assessments that treated Wayland insertion as only `wtype`-based, or that said direct insertion was not working at all, are now stale.

The broader product conclusion still does not become "works everywhere", because:

- the diagnostics and UI now distinguish capability much better, but support still depends on the active backend and desktop
- the codebase still allows fallback delivery to count as a successful result category
- desktop and host-tool dependencies still determine whether real typing is available

## Current Code Smells

### 1. UI orchestration is still too stateful

`src/ui/main_window.rs` has improved, but it still owns a large amount of application flow:

- dictation start/stop state
- host signal consumption
- transcript presentation
- clipboard fallback
- keyboard shortcuts

This is manageable for now, but the UI layer still knows too much about host behavior and insertion outcomes.

### 2. Async integration is polling-heavy, though cleaner than before

GTK-facing async work is still bridged through timer polling. A shared helper now exists in `src/ui/async_poll.rs`, which is an improvement, but the architecture still depends on:

- worker thread
- channel
- periodic GTK poll

This is not broken, but it is still a code smell because state transitions remain spread across multiple timers instead of a single event model.

### 3. Host D-Bus endpoints own business logic

`src/bin/saywrite-host/dbus.rs` is doing transport, dictation control, insertion, state tracking, and signal emission. That makes it harder to:

- test behavior in isolation
- change insertion policy safely
- distinguish backend errors from orchestration errors

### 4. Error handling is still partly string-based

Some host errors are typed now, but the UI still interprets failures by matching message substrings. That creates drift risk between backend behavior and frontend messaging.

### 5. Runtime capability reporting is better, but still not the full product truth

The repo now has explicit insertion capability and result categories, which is a real improvement. The remaining issue is that product-level messaging can still overgeneralize from the active setup.

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
- clipboard tools

This is practical, but not robust enough to support strong claims like "works in any text field".

### 7. Test coverage is improving, but still narrow overall

There is now some real host-side unit coverage, including backend classification and IBus parsing. The remaining thin areas are:

- host D-Bus behavior
- settings migration and runtime probing
- end-to-end host-side dictation state transitions
- IBus engine restore and commit lifecycle under failure

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

Those paths can still return success-like messages, which means the system can report successful delivery even when it only copied text or showed a notification.

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

## What Should Change

### 1. Make insertion capability honest

Introduce explicit capability states such as:

- `Typing`
- `ClipboardOnly`
- `NotificationOnly`
- `Unavailable`

The UI should stop presenting clipboard fallback as equivalent to successful direct insertion.

### 2. Separate "typed" from "delivered"

Host responses and UI copy should distinguish:

- text typed into target field
- text copied to clipboard
- text shown in notification

These are different outcomes and should not share the same success category.

### 3. Keep backend reporting explicit in diagnostics

This is now implemented in the host/runtime path and should remain explicit in the UI and support docs, for example:

- `ibus-engine`
- `wtype`
- `xdotool`
- `clipboard only`
- `notification only`

### 4. Narrow the product promise

The app should not claim "dictate into any text field" unless a real typing backend is active and the current desktop path supports that claim.

### 5. Add targeted tests for insertion policy and host flow

The next tests should focus on:

- host D-Bus state transitions
- IBus engine restore behavior
- fallback behavior
- host insertion result reporting

### 6. Add verification around the IBus bridge

The IBus bridge is now part of the working insertion story, so the next step is not "get it compiling" but "prove it stays working":

- add focused tests where practical
- add repeatable manual verification notes for supported desktops
- report when the bridge is registered versus when SayWrite is only falling back

## Recommended Product Language

Today the honest promise is closer to:

- "Dictate, clean, and copy anywhere."
- "Direct typing works on supported host setups, including the current IBus-based GNOME Wayland path."

It is still not accurate to promise:

- "Live dictation into any text field on Linux."

## Bottom Line

SayWrite is already useful because transcription and clipboard flow work well, and direct insertion is now working on supported setups. The new IBus path is a meaningful improvement to the Wayland story. But the current host insertion layer still does not justify the stronger UX promise of universal direct text-field insertion across Linux.

The main issue is not a single bug. It is a mismatch between:

- what the insertion layer can really do on the current host setup
- what the diagnostics report
- what the product currently implies

The next meaningful improvement is still not another transcription change. It is:

- keeping insertion capability explicit and truthful
- separating real typing success from clipboard or notification delivery
- documenting and verifying the supported insertion environments
