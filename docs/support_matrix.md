# SayWrite Support Matrix

This document defines what SayWrite should explicitly test and what it can honestly claim for a first public release.

Use this together with `next_steps.md`.

## Release Position

For the first public beta, SayWrite should not claim universal Linux support.

It should claim:

- supported direct typing on validated setups
- clear degraded modes on unsupported setups
- hotkey-first dictation on supported native builds

## Support Levels

### Supported

These setups have been tested end to end and are acceptable to mention in release notes and onboarding copy.

Requirements:

- hotkey starts dictation
- hotkey stops dictation
- cleaned text lands in the focused field directly
- repeated dictation works without restarting the daemon
- diagnostics report the active backend honestly

### Degraded

These setups are usable, but only with clipboard or notification fallback.

Requirements:

- hotkey dictation still works
- transcript is delivered clearly through the fallback path
- the app explains that direct typing is not active

### Unsupported

These setups should not be claimed for release.

Examples:

- no functioning direct-typing path
- no usable insertion backend
- repeated dictation wedges the runtime
- input method or desktop path is unverified and unreliable

## Environment Matrix

Track each row as `Pass`, `Degraded`, `Fail`, or `Untested`.

| Desktop/session | Insertion path | Status | Notes |
|---|---|---|---|
| GNOME Wayland | IBus engine | Pass on current machine | Direct insertion proven locally |
| GNOME Wayland | IBus engine on second machine/account | Untested | Needed before broader beta claim |
| X11 | xdotool | Untested | Important secondary target |
| KDE Plasma Wayland | XDG GlobalShortcuts portal + wtype/ydotool | Untested | Portal likely works; insertion needs validation |
| wlroots Wayland | wtype | Untested | Good non-GNOME Wayland target |
| Wayland unsupported compositor | clipboard / notification | Untested | Should degrade clearly |

## App Matrix

Each supported environment should be tested against representative app types.

| App type | Example | Goal | Status |
|---|---|---|---|
| Browser textarea | Firefox / Chromium | Direct insertion into web text box | Untested |
| GTK app | Text Editor / Builder | Native Linux text input | Untested |
| Qt app | Kate / Qt Creator | Non-GTK desktop app | Untested |
| Electron app | VS Code / Discord / Slack | Common real-world target | Untested |
| Terminal/chat input | Codex / shell-based tools | Fast workflow dictation | Untested |

## Core Test Script

Run this for every matrix row:

1. Start SayWrite.
2. Confirm diagnostics show the expected insertion mode.
3. Focus a text field in the target app.
4. Press the hotkey once to start.
5. Speak a short sentence with punctuation.
6. Press the hotkey once to stop.
7. Verify the cleaned text lands in the expected place.
8. Repeat twice in the same field.
9. Try a quick accidental repeat or held-key scenario.
10. Confirm the runtime returns to `idle`.

## Current Test TODO

Run these on the current GNOME Wayland machine and record each row as `Pass`, `Degraded`, `Fail`, or `Untested`.

### App Checklist

- [ ] Browser textarea: Firefox or Chromium
- [ ] GTK app: Text Editor
- [ ] Qt app: Kate or another Qt text editor
- [ ] Electron app: VS Code, Discord, or Slack
- [ ] Terminal/chat input: Codex or another terminal-based prompt/chat tool

### Repeatability Checklist

- [ ] Repeat dictation twice in the same field
- [ ] Try a quick accidental repeat press
- [ ] Try a short held-key scenario and confirm the mic does not wedge
- [ ] Confirm the runtime returns to `idle` after each run

### Expected Phrase

Use this phrase for consistency:

`hello from saywrite question mark`

Expected cleaned result:

`Hello from SayWrite?`

### Log Watch

Keep stderr or terminal logs visible while testing.

Healthy run:

- `ToggleDictation start ok: Listening...`
- `ToggleDictation stop ok: raw_len=... cleaned_len=...`
- `ToggleDictation insertion result: ok=true kind=typed ...`

Bad signs:

- repeated `A dictation session is already running.`
- stuck microphone indicator
- insertion result `ok=false`
- runtime not returning to `idle`

## Beta Release Gate

Before public beta, these must be true:

- GNOME Wayland direct insertion passes on at least two real setups
- X11 direct insertion passes on at least one real setup
- one degraded fallback path is verified and clearly reported
- held-key and repeated-toggle bugs do not wedge the runtime
- `cargo test` passes
- `cargo clippy --all-targets --all-features -- -D warnings` passes

## Nice To Have Before Beta

- one wlroots Wayland pass with `wtype`
- one KDE Plasma Wayland pass with GlobalShortcuts portal
- screenshots of the diagnostics states for supported and degraded modes
- short release notes listing supported and degraded environments

## Non-Goals For First Beta

These should not block the first release:

- every Linux compositor
- every app category
- perfect hands-free or press-and-hold semantics
- tray UX
- broad marketing claims like "works in any text field on Linux"

## Current Best Claim

Today, the strongest honest product claim is:

"SayWrite is a hotkey-first Linux dictation app with direct typing on validated setups and clear fallback delivery elsewhere."
