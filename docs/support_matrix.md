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
- repeated dictation works without restarting the app
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

Track each row as `Pass`, `Degraded`, `Fail`, or `Untested`. A row only moves to `Pass` after a native package install is tested on that actual desktop/session, the Diagnostics page matches the expected mode, and the Core Test Script below passes in at least one real app.

| Desktop/session | Insertion path | Status | Notes |
|---|---|---|---|
| GNOME Wayland | IBus engine | Pass on current machine | Browser, GTK, Electron, terminal/chat, and repeatability checks passed locally |
| GNOME Wayland | IBus engine on second machine/account | Untested | Needed before broader beta claim |
| X11 | xdotool | Untested | Important secondary target |
| KDE Plasma Wayland | XDG GlobalShortcuts portal + wtype/ydotool | Untested | Portal likely works; insertion needs validation |
| wlroots Wayland | wtype | Untested | Good non-GNOME Wayland target |
| Wayland unsupported compositor | clipboard / notification | Untested | Should degrade clearly |

## Native Validation Runbook

Use this runbook for every environment row. Record the date, distro, desktop, session type, package version, insertion status, hotkey status, and tested apps before changing a row's support level.

### 1. Build the Package

Run from the repo checkout:

```bash
cargo check
cargo test
cargo deb
```

Expected package:

```text
target/debian/saywrite_0.3.5-1_amd64.deb
```

### 2. Install the Native Package

Install the newly built package before testing installed behavior:

```bash
sudo apt install --reinstall ./target/debian/saywrite_0.3.5-1_amd64.deb
```

After installing, confirm the package does not ship old companion runtime files:

```bash
dpkg -L saywrite | rg 'saywrite-host|io.github.saywrite.Host.service|systemd/user'
```

Expected result: no matches.

### 3. Capture Environment Evidence

Record these values with the test result:

```bash
date --iso-8601=seconds
cat /etc/os-release
echo "$XDG_CURRENT_DESKTOP"
echo "$XDG_SESSION_TYPE"
command -v ibus
command -v wtype
command -v xdotool
```

Only the dependency for the active desktop/session has to exist. For example, X11 validation needs `xdotool`; non-GNOME Wayland validation needs `wtype`; GNOME Wayland validation needs IBus support.

### 4. Launch and Check Diagnostics

Start SayWrite from the desktop launcher or with:

```bash
/usr/bin/saywrite
```

Open Settings, then Diagnostics. Confirm:

- `Desktop session` matches the actual session.
- `Insertion` matches the expected mode for this environment.
- `Desktop checks` does not show missing packages for the expected Direct Typing path.
- `Runtime` says Direct Typing is built into the native app.

### 5. Verify Shortcut Ownership

While SayWrite is running, check the compatibility interface used by the packaged shortcut:

```bash
busctl --user status io.github.saywrite.Host
```

Expected result: `/usr/bin/saywrite` owns the bus name. If another process owns it, the row is `Fail` until the stale runtime is removed.

### 6. Run the Core Test Script

Use the Core Test Script and App Matrix below. For each app, record:

- delivery result: typed, copied, notified, or failed
- whether repeated dictation works
- whether a quick repeated hotkey press leaves the runtime idle
- any app-specific insertion quirks

### 7. Classify the Row

Use these rules:

- `Pass`: Direct Typing works end to end, repeatability passes, diagnostics are accurate.
- `Degraded`: Clipboard Mode or notification delivery works and the app reports Direct Typing as unavailable or unsupported.
- `Fail`: dictation wedges, diagnostics lie, insertion fails without clear fallback, or stale compatibility ownership prevents the packaged app from running correctly.
- `Untested`: no real package install has been tested for that row.

## App Matrix

Each supported environment should be tested against representative app types.

| App type | Example | Goal | Status |
|---|---|---|---|
| Browser textarea | Firefox / Chromium / Brave | Direct insertion into web text box | Pass on current GNOME Wayland machine |
| GTK app | Text Editor / Builder | Native Linux text input | Pass on current GNOME Wayland machine |
| Qt app | Kate / Qt Creator | Non-GTK desktop app | Untested |
| Electron app | VS Code / Discord / Slack | Common real-world target | Pass on current GNOME Wayland machine |
| Terminal/chat input | Codex / shell-based tools | Fast workflow dictation | Pass on current GNOME Wayland machine |

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

Native package smoke validation on April 24, 2026:

- [x] Rebuilt and reinstalled `target/debian/saywrite_0.3.5-1_amd64.deb`
- [x] Confirmed installed package does not contain `saywrite-host`, user systemd service, or D-Bus activation assets
- [x] Confirmed startup removes stale user-local `saywrite-host` files from previous installs
- [x] Confirmed `io.github.saywrite.Host` is owned by `/usr/bin/saywrite`
- [x] Confirmed diagnostics report `hotkey_active=true`, Direct Typing `typing`, and IBus backend on the current GNOME Wayland session

Package smoke refresh on April 24, 2026 at 22:48 BST:

- Environment: Zorin OS 18.1 (`ID=zorin`, `ID_LIKE="ubuntu debian"`), `XDG_CURRENT_DESKTOP=zorin:GNOME`, `XDG_SESSION_TYPE=wayland`
- Dependency probes: `ibus=/usr/bin/ibus`, `wtype=/usr/bin/wtype`, `xdotool` missing as expected for the current GNOME Wayland row
- Checks passed: `cargo fmt --check`, `cargo check`, `cargo test`, `cargo deb`
- Reinstalled package: `target/debian/saywrite_0.3.5-1_amd64.deb`
- Package content check: no `saywrite-host`, `io.github.saywrite.Host.service`, or `systemd/user` files
- Runtime ownership: `/usr/bin/saywrite` owns `io.github.saywrite.Host`
- Runtime status: `typing`, backend `ibus-engine`, hotkey active
- GNOME shortcut fallback: `Super+Alt+D` runs `/usr/bin/saywrite-dictation.sh`
- Manual focused-input result: Direct Typing delivered text into the active chat/input field
- Spoken phrase result observed: `Hello from Say Write?` followed by additional dictated text
- Follow-up cleanup note: direct insertion passed, but the brand phrase can still transcribe as `Say Write` instead of `SayWrite`
- Remaining manual work for this row: complete browser, GTK, Qt, Electron, and repeatability checks

GNOME Wayland app-matrix refresh on April 26, 2026 at 17:44 BST:

- Environment: Zorin OS 18.1 (`ID=zorin`, `ID_LIKE="ubuntu debian"`), `XDG_CURRENT_DESKTOP=zorin:GNOME`, `XDG_SESSION_TYPE=wayland`
- Checks passed before testing: `cargo fmt --check`, `cargo check`, `cargo test`, `cargo deb`
- Reinstalled package: `target/debian/saywrite_0.3.5-1_amd64.deb`
- Package content check: no `saywrite-host`, `io.github.saywrite.Host.service`, or `systemd/user` files
- Runtime ownership: `/usr/bin/saywrite` owns `io.github.saywrite.Host`
- Runtime status before manual tests: Direct Typing `typing`, backend `ibus-engine`, hotkey active
- Manual app results reported by tester: Brave local textarea, GNOME Text Editor, VS Code, and terminal/chat input all passed with Direct Typing
- Repeatability result reported by tester: repeated dictation, quick repeat, and held-key checks passed without wedging the mic/runtime
- Log evidence: five successful `kind=typed` IBus commits were captured during the run
- Observed edge case: one final empty-capture run produced `raw_len=0` and `kind=failed` with `SayWrite hit an unexpected error`; no stuck microphone or repeated-session wedge was reported
- Remaining manual work for this row: Qt app target remains untested because no Qt text editor was installed locally

Run these on the current GNOME Wayland machine and record each row as `Pass`, `Degraded`, `Fail`, or `Untested`.

### App Checklist

- [x] Browser textarea: Brave local textarea
- [x] GTK app: Text Editor
- [ ] Qt app: Kate or another Qt text editor
- [x] Electron app: VS Code
- [x] Terminal/chat input: Codex or another terminal-based prompt/chat tool

### Repeatability Checklist

- [x] Repeat dictation twice in the same field
- [x] Try a quick accidental repeat press
- [x] Try a short held-key scenario and confirm the mic does not wedge
- [x] Confirm the runtime returns to an available state after each run

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

## Next Validation Targets

Validate in this order:

1. **X11 + xdotool**: primary secondary target because it covers Ubuntu/Zorin users who still choose X11 sessions.
2. **KDE Plasma Wayland + wtype**: validates the non-GNOME Wayland path and the XDG GlobalShortcuts portal on a mainstream desktop.
3. **wlroots Wayland + wtype**: validates the same insertion backend against a different compositor family.
4. **Degraded Wayland fallback**: intentionally test a session without a Direct Typing backend and confirm Clipboard Mode is clearly reported.

Do not mark KDE or wlroots as supported until the packaged app is tested from a clean user account or VM and the result is recorded here.

## Beta Release Gate

Before public beta, these must be true:

- GNOME Wayland direct insertion passes on at least two real setups
- X11 direct insertion passes on at least one real setup
- non-GNOME Wayland is either validated for Direct Typing or explicitly documented as degraded
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
