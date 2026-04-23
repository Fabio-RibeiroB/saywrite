# SayWrite Next Steps

SayWrite has crossed the main technical hurdle: hotkey-driven dictation, cleanup, and direct insertion now work on a real GNOME Wayland machine through the in-process native runtime and IBus bridge.

The next phase is simplifying distribution and reducing architectural complexity by moving from Flatpak-first to `.deb`-first, targeting Ubuntu/Debian-based distros.

## Current State

- The GTK app exists with onboarding, main dictation window, settings, diagnostics, and shortcut capture.
- The app now owns the real dictation workflow on native builds.
- Global hotkey dictation works through the host path while SayWrite is running.
- Local (whisper.cpp) transcription works end to end.
- Cloud transcription works with OpenAI-compatible APIs.
- Direct insertion works on the currently validated GNOME Wayland setup via IBus bridge.
- `wtype` (Wayland) and `xdotool` (X11) insertion paths exist but are untested on real hardware.
- Clipboard and notification fallbacks exist for degraded environments.
- The app exposes a compatibility D-Bus interface so existing GNOME fallback launchers still work during migration.
- Shortcut capture dialog temporarily suspends the active GNOME keybinding so all key combos can be captured.
- Shortcut changes update GNOME keybindings directly through native `gsettings` calls.
- Desktop detection auto-selects the best insertion backend per session (GNOME Wayland, Other Wayland, X11, Other).
- Insertion capability and result kinds are explicit: `typing`/`clipboard-only`/`notification-only`/`unavailable` and `typed`/`copied`/`notified`/`failed`.
- Host-side toggle debounce (900ms) prevents repeated shortcut activations from wedging the daemon.
- Error sanitization maps typed `DictationError` variants to user-friendly messages.
- Host-side unit tests cover backend classification, result-kind mapping, IBus parsing, error sanitization, and toggle debounce.

## Migration: Flatpak → `.deb`-First

### Why

The Flatpak sandbox forces an app/host split that adds significant complexity:

- **`saywrite-host` companion** — a separate daemon just to escape the sandbox
- **D-Bus IPC between app and host** — extra failure surface
- **`flatpak-spawn --host`** — for settings sync, shortcut registration, host lifecycle
- **Host daemon lifecycle** — start on launch, stop + mask on close, D-Bus name ownership checks
- **Stale version issues** — GNOME caches old Flatpak builds, making dogfeeding painful

A native `.deb` removes the sandbox boundary entirely. The app runs directly on the host and can:

- Call D-Bus, IBus, and desktop APIs without a companion daemon
- Register global shortcuts directly
- Insert text without D-Bus round-trips
- Be tested with `cargo run` — no rebuild/reinstall cycle
- Update via `apt` with no stale cache

### What Gets Simplified

| Flatpak Architecture | `.deb` Architecture |
|---|---|
| GTK app + `saywrite-host` daemon | Single binary |
| D-Bus IPC between app and host | Direct function calls |
| `flatpak-spawn --host` for everything | Native system calls |
| XDG portal or `gsettings` for shortcuts | `libkeybinder` or direct D-Bus |
| Host install flow in Settings | No install flow needed |
| Host lifecycle management in `app.rs` | Gone |
| `host_setup.rs` desktop detection | Still useful, but simpler |
| `host_integration.rs` D-Bus client | Direct D-Bus or IBus calls |
| `insertion.rs` in host daemon | Moved into the app |
| `input.rs` in host daemon | Moved into the app |

### What Stays the Same

- Transcription logic (`dictation.rs`, whisper.cpp, cloud API)
- Cleanup (`cleanup_transcript`)
- Settings (`config.rs`) — just no Flatpak-to-host sync
- UI components (onboarding, main window, preferences, shortcut capture)
- Desktop detection — still useful for choosing insertion backend
- Capability reporting — still useful for honest diagnostics

### Tradeoffs and Risks

- **GTK4/libadwaita requires Ubuntu 24.04+** — older distros ship GTK 3.x. If broader coverage is needed, drop libadwaita or switch toolkit.
- **No sandbox** — a dictation app with mic access running unsandboxed may concern some users. This needs honest messaging.
- **Multiple distro versions** — Ubuntu 22.04, 24.04, 24.10, Debian 11, 12 each have different library versions. Need separate `.deb` builds per target, or use Launchpad's build service.
- **Global shortcuts on Wayland are still hard** — the sandbox isn't the only blocker. Wayland itself restricts global keybinding registration. You still need the XDG GlobalShortcuts portal or IBus tricks, just without the `flatpak-spawn` layer.
- **Updates without a PPA** — users manually download new `.deb` files. Setting up a PPA adds infrastructure overhead (package signing, Launchpad maintenance).
- **whisper.cpp packaging** — bundle it in the `.deb` (larger package) or make it a dependency (users install separately).

### Migration Path

1. **Phase 1: `.deb` alongside Flatpak** — Build a `.deb` with `cargo-deb`. Keep the Flatpak. Use the `.deb` for dogfeeding and faster iteration.
2. **Phase 2: Merge host into app** — Move `insertion.rs`, `input.rs`, and D-Bus service logic into the main app. Remove `saywrite-host` binary.
3. **Phase 3: Remove Flatpak-specific code** — Strip `flatpak-spawn`, host lifecycle, D-Bus IPC client. Simplify `app.rs`, `host_setup.rs`, `host_integration.rs`.
4. **Phase 4: PPA (optional)** — Set up a Launchpad PPA for automatic `apt` updates once the product stabilizes.
5. **Phase 5: Flatpak as optional** — Keep Flatpak for users who want sandboxing, but it's no longer the primary distribution channel.

## Cross-Desktop Compatibility

The goal: any Ubuntu/Debian user should get working dictation with minimal friction.

### Global Shortcut

Currently the global hotkey relies on:
1. XDG GlobalShortcuts portal (registered by `saywrite-host` at startup)
2. GNOME custom keybindings via `gsettings`

**After migration:**

- Use `libkeybinder` for X11 global hotkeys (simple, well-supported)
- Use XDG GlobalShortcuts portal for Wayland (works on KDE, wlroots; GNOME support is inconsistent)
- GNOME fallback: set custom keybinding via `gsettings` directly (no `flatpak-spawn` needed)
- Show clear in-app instructions when no automatic path is available

### Text Insertion

- **GNOME Wayland**: IBus bridge (already implemented, works)
- **Other Wayland**: `wtype` — validate on real hardware
- **X11**: `xdotool type` — validate on real hardware
- **Clipboard fallback**: Already works everywhere

### Detection and Honest Reporting

- Auto-detect the desktop environment and session type at startup ✅ (implemented in `host_setup.rs`)
- Report the actual insertion method in the UI ✅ (implemented via capability labels)
- Don't show GNOME-specific setup steps on KDE or other desktops — needs audit
- Tailor onboarding copy to the detected environment — needs work

## Product Framing

The product is presented as two user-facing modes:

- `Clipboard Mode`:
  - works on any desktop
  - records, transcribes, cleans, and copies text
  - is the safe default when direct typing is not enabled
- `Direct Typing Mode`:
  - places text directly into the focused application
  - availability depends on the desktop environment and session type

The key UX goal is that users experience one product, not "app plus helper".

## Completed Milestones

### Package the Host Companion Cleanly ✅
- In-app installation via `host_setup::install_host_companion()` works end-to-end
- Settings shows Clipboard Mode vs Direct Typing clearly
- Host daemon lifecycle tied to GUI (starts on launch, stops and is masked on close)
- Host refuses to start without app D-Bus name ownership

### Harden the IBus Path ✅
- Fixed race condition, added retry logic, comprehensive logging
- Engine swap/restore with 120ms delay and retry on failure
- Repeated dictation reliable without daemon restart

### Capability Reporting ✅
- Runtime probing complete: GPU detection, whisper.cpp discovery, local model check
- Diagnostics page shows insertion backend and hotkey status
- Onboarding shows real mode from host_status
- Explicit capability states: `typing`, `clipboard-only`, `notification-only`, `unavailable`
- Explicit result kinds: `typed`, `copied`, `notified`, `failed`

### Host Regression Tests ✅
- Backend classification, result-kind mapping, IBus parsing
- Service error sanitization + debounce tests

### UI Polish ✅
- WaveformBox, Revealers, inline setup actions, insertion chip
- Shortcut capture with GNOME keybinding suspend/restore
- Onboarding shortcut page with explicit Change button
- Settings sync from Flatpak to host via `flatpak-spawn --host`

### Desktop Detection ✅
- `host_setup.rs` detects GNOME Wayland, Other Wayland, X11, Other
- Per-profile dependency checks with Ubuntu/Zorin package hints
- Diagnostics show desktop label, host files status, and dependency status

## Release Priorities

Slices 1-3 on `deb-first` are complete:

- `.deb` packaging is wired up with `cargo-deb`
- the direct-typing runtime has been pulled into the app
- Flatpak-specific runtime behavior has been removed from the native path

The next work is cleanup and deletion of migration leftovers, not another transport rewrite.

## `deb-first` Branch Refactor Map

This branch is the native packaging and architecture migration branch. It stays in the same repo; the goal is to prove the Debian-first path without forking the project or carrying two long-term runtime models.

### Target Shape

- Primary distribution: native `.deb` for Ubuntu/Zorin first
- Primary runtime: single native GTK app process
- No Flatpak-first assumptions in the main code path
- No required `saywrite-host` companion for the Debian build
- Keep direct typing honest: supported where validated, degraded where not

### Refactor Slices

#### Slice 1: Native packaging without changing behavior

Goal: ship a `.deb` quickly so day-to-day dogfooding stops depending on Flatpak.

Files/modules affected:

- [Cargo.toml](/home/fabio/Documents/GitHub/saywrite/Cargo.toml): add `cargo-deb` metadata
- [README.md](/home/fabio/Documents/GitHub/saywrite/README.md): make `.deb` the primary install path
- [data/io.github.fabio.SayWrite.appdata.xml](/home/fabio/Documents/GitHub/saywrite/data/io.github.fabio.SayWrite.appdata.xml): release messaging for `.deb`
- [scripts/install-host.sh](/home/fabio/Documents/GitHub/saywrite/scripts/install-host.sh): keep temporarily for transition only

Deliverable:

- a native `.deb` that installs the current app cleanly on Ubuntu/Zorin
- Flatpak remains available, but is no longer the default dogfooding path

#### Slice 2: Pull host logic into the app

Goal: remove the artificial app/host boundary from the Debian build.

Source modules to absorb:

- [src/bin/saywrite-host/input.rs](/home/fabio/Documents/GitHub/saywrite/src/bin/saywrite-host/input.rs)
- [src/bin/saywrite-host/insertion.rs](/home/fabio/Documents/GitHub/saywrite/src/bin/saywrite-host/insertion.rs)
- [src/bin/saywrite-host/service.rs](/home/fabio/Documents/GitHub/saywrite/src/bin/saywrite-host/service.rs)

Code that becomes transitional or removable:

- [src/host_integration.rs](/home/fabio/Documents/GitHub/saywrite/src/host_integration.rs)
- [src/host_api.rs](/home/fabio/Documents/GitHub/saywrite/src/host_api.rs)
- [src/bin/saywrite-host/dbus.rs](/home/fabio/Documents/GitHub/saywrite/src/bin/saywrite-host/dbus.rs)
- [src/bin/saywrite-host/main.rs](/home/fabio/Documents/GitHub/saywrite/src/bin/saywrite-host/main.rs)

Implementation direction:

- create an in-process integration controller in the main app
- keep capability/result enums, but stop transporting them over D-Bus
- replace D-Bus signal subscription with direct event delivery inside the app
- keep desktop-specific insertion backends; only remove the transport boundary

#### Slice 3: Remove Flatpak-specific runtime behavior

Goal: stop designing the app around the sandbox.

Files/modules affected:

- [src/app.rs](/home/fabio/Documents/GitHub/saywrite/src/app.rs): remove `systemctl` start/stop/mask lifecycle
- [src/config.rs](/home/fabio/Documents/GitHub/saywrite/src/config.rs): remove Flatpak host settings sync and install-id assumptions
- [src/host_setup.rs](/home/fabio/Documents/GitHub/saywrite/src/host_setup.rs): remove host install flow, host path probing, and `flatpak-spawn --host`
- [flatpak/io.github.fabio.SayWrite.json](/home/fabio/Documents/GitHub/saywrite/flatpak/io.github.fabio.SayWrite.json): demote, then remove when the branch lands

What stays:

- desktop/session detection
- dependency detection and package hints
- GNOME shortcut self-heal logic, but using native calls only

#### Slice 4: Simplify UI copy and setup flows

Goal: remove product language that only exists because of Flatpak.

Files/modules affected:

- [src/ui/preferences.rs](/home/fabio/Documents/GitHub/saywrite/src/ui/preferences.rs)
- [src/ui/onboarding.rs](/home/fabio/Documents/GitHub/saywrite/src/ui/onboarding.rs)
- [src/runtime.rs](/home/fabio/Documents/GitHub/saywrite/src/runtime.rs)
- [src/ui/main_window/widgets.rs](/home/fabio/Documents/GitHub/saywrite/src/ui/main_window/widgets.rs)

Changes:

- remove "install host companion" actions from Settings
- replace "host daemon" wording with native direct-typing diagnostics
- keep Clipboard Mode vs Direct Typing Mode as the user-facing model
- show backend-specific limitations without mentioning sandbox internals

#### Slice 5: Delete the old architecture

Goal: make the native path the only primary architecture.

To remove once the native path is working:

- `saywrite-host` binary target
- user systemd service and D-Bus activation files in `data/`
- host install scripts that only exist for Flatpak
- `flatpak-spawn --host` code paths
- Flatpak-first documentation

### Sequence

1. Add `.deb` packaging so dogfooding moves off Flatpak immediately.
2. Introduce an in-process integration controller and port host logic into it.
3. Switch UI/runtime code from `host_*` APIs to the native controller.
4. Remove the app-managed host lifecycle and install flow.
5. Delete the obsolete daemon, D-Bus, and Flatpak-specific files.
6. Re-run support-matrix validation on native builds before calling the migration done.

### Guardrails

- Do not promise universal Linux support just because the sandbox is gone.
- Keep GNOME Wayland as the primary validated path until X11/KDE/wlroots are actually tested.
- Preserve Clipboard Mode as the safe fallback.
- Avoid a second long-lived architecture; native becomes primary, Flatpak is either degraded or removed.

### v0.4 — `.deb` Packaging
1. Publish a Debian dev package for Ubuntu/Zorin dogfooding
2. Validate native install on Ubuntu 24.04 or Zorin OS
3. Make `.deb` the primary internal testing path

### v0.5 — Merge Host Into App
1. Remove `saywrite-host` binary and systemd service from the supported native path
2. Delete remaining migration-era compatibility code and stale copy
3. Simplify `host_api.rs`, `host_integration.rs`, and related package assets where they only exist for compatibility

### v1.0 — Polish and PPA
1. PPA setup for automatic `apt` updates
2. Tray icon and quick controls
3. Custom vocabulary and context hints
4. Move UI away from substring error matching toward typed error handling
5. Consolidate async state model (timer polling → event-driven)
6. Cross-desktop validation on non-GNOME setups

## Non-Blocking

- More aggressive cleanup customization
- Application-aware formatting profiles
