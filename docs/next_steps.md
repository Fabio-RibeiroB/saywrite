# SayWrite Next Steps

SayWrite has crossed the main technical hurdle: hotkey-driven dictation, cleanup, and direct insertion now work on a real GNOME Wayland machine through the host daemon and IBus bridge.

The next phase is simplifying distribution and reducing architectural complexity by moving from Flatpak-first to `.deb`-first, targeting Ubuntu/Debian-based distros.

## Current State

- The GTK app exists with onboarding, main dictation window, settings, diagnostics, and shortcut capture.
- `saywrite-host` owns the real dictation workflow (D-Bus service, IBus bridge, GlobalShortcuts portal).
- Global hotkey dictation works through the host path while SayWrite is running.
- Local (whisper.cpp) transcription works end to end.
- Cloud transcription works with OpenAI-compatible APIs.
- Direct insertion works on the currently validated GNOME Wayland setup via IBus bridge.
- `wtype` (Wayland) and `xdotool` (X11) insertion paths exist but are untested on real hardware.
- Clipboard and notification fallbacks exist for degraded environments.
- Host daemon lifecycle is tied to the GUI (starts on app launch, stops and is masked on close).
- `saywrite-host` refuses to start unless the app owns its D-Bus name, preventing orphan daemons.
- Shortcut capture dialog temporarily suspends the active GNOME keybinding so all key combos can be captured.
- Shortcut changes from within Flatpak update GNOME keybindings directly via `flatpak-spawn --host gsettings`.
- In-app host installation with progress feedback (builds release binary if needed).
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

### v0.4 — `.deb` Packaging
1. Set up `cargo-deb` for `.deb` builds
2. Publish `.deb` alongside Flatpak on GitHub releases
3. Validate `.deb` install on Ubuntu 24.04
4. Use `.deb` for dogfeeding (faster iteration, no stale cache)

### v0.5 — Merge Host Into App
1. Move `insertion.rs` and `input.rs` from host daemon into the app
2. Replace D-Bus IPC with direct function calls
3. Remove `saywrite-host` binary and systemd service
4. Simplify `app.rs` (no host lifecycle), `host_integration.rs` (no D-Bus client), `host_setup.rs` (no install flow)
5. Keep Flatpak as optional for sandbox users

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
