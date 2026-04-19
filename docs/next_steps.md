# SayWrite Next Steps

SayWrite has crossed the main technical hurdle: hotkey-driven dictation, cleanup, and direct insertion now work on a real GNOME Wayland machine through the host daemon and IBus bridge.

The next phase is cross-desktop compatibility. SayWrite should work out of the box for anyone who can install a Flatpak, regardless of desktop environment.

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

## Cross-Desktop Compatibility — Priority #1

The goal: any Linux user who installs the Flatpak should get working dictation with minimal friction. GNOME, KDE Plasma, XFCE, Sway, Hyprland — all should work.

### Global Shortcut (the biggest gap)

Currently the global hotkey relies on two mechanisms:
1. XDG GlobalShortcuts portal (registered by `saywrite-host` at startup — works on KDE and some wlroots compositors)
2. GNOME custom keybindings via `gsettings` (fallback set by `install-gnome-shortcut.sh`)

**What needs to happen:**

- **KDE Plasma**: The XDG GlobalShortcuts portal works well on KDE. Test and validate that `saywrite-host`'s portal registration works out of the box. If it does, KDE users get zero-setup hotkeys.
- **wlroots (Sway, Hyprland)**: These compositors generally support the XDG GlobalShortcuts portal. Validate and document.
- **XFCE / X11 desktops**: No portal support. Provide a fallback: detect the desktop and offer instructions or a script to set up a custom shortcut that calls `busctl --user call io.github.saywrite.Host /io/github/saywrite/Host io.github.saywrite.Host ToggleDictation`.
- **Generic fallback UI**: When no portal is available and no GNOME keybinding is detected, show clear in-app instructions for the user to set up a system shortcut manually. Include a "Test Shortcut" button so they can verify it works.
- **Portal-first strategy**: Make the XDG GlobalShortcuts portal the primary path on all desktops. Only fall back to GNOME gsettings when the portal is unavailable.

### Text Insertion

Currently insertion is GNOME-specific (IBus bridge). Other desktops need their own paths:

- **KDE Plasma Wayland**: Investigate `ydotool`, `wtype`, or KDE's input method framework
- **wlroots Wayland**: `wtype` is already probed by `insertion.rs` — validate on real hardware
- **X11 (any desktop)**: `xdotool type` is already probed — validate on real hardware
- **Clipboard fallback**: Already works everywhere — this is the safe default and should remain seamless

### Detection and Honest Reporting

- Auto-detect the desktop environment and session type at startup ✅ (implemented in `host_setup.rs`)
- Report the actual insertion method in the UI (Direct Typing / Clipboard / Notification) ✅ (implemented via capability labels)
- Don't show GNOME-specific setup steps on KDE or other desktops — needs audit
- Tailor onboarding copy to the detected environment — needs work

### Concrete Tasks

1. **Test XDG GlobalShortcuts portal on KDE Plasma** — if it works, KDE gets first-class support with no code changes
2. **Test `wtype` insertion on Sway/Hyprland** — already probed, just needs validation
3. **Test `xdotool` insertion on X11** — already probed, just needs validation
4. **Add desktop-aware shortcut setup** — detect DE and use the right mechanism (portal → gsettings → manual instructions)
5. **Add a "Test Shortcut" flow** — after setting up a shortcut (manually or via portal), let the user verify it triggers dictation before leaving setup
6. **Remove GNOME assumptions from UI copy** — audit all user-facing strings for GNOME-specific language
7. **Update `apply_shortcut_change`** — use portal or DE-specific mechanism instead of always trying GNOME gsettings

## Product Framing

The product is presented as two user-facing modes:

- `Clipboard Mode`:
  - works with the Flatpak app alone on any desktop
  - records, transcribes, cleans, and copies text
  - is the safe default when direct typing is not enabled
- `Direct Typing Mode`:
  - enables host integration outside the sandbox
  - allows hotkey-driven dictation and text insertion into supported host apps
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

### v1.1 — Cross-Desktop Support
1. Validate XDG GlobalShortcuts portal on KDE Plasma and wlroots
2. Validate `wtype` and `xdotool` insertion on non-GNOME desktops
3. Desktop-aware shortcut setup (portal-first, DE fallbacks)
4. Audit and fix all GNOME-specific UI copy
5. "Test Shortcut" verification flow
6. GNOME Wayland direct insertion validation on a second machine/account

### v1.2 — Polish
- Tray icon and quick controls
- Custom vocabulary and context hints
- Application-aware formatting profiles
- Move UI away from substring error matching toward typed error handling
- Consider consolidating async state model (timer polling → event-driven)

## Non-Blocking

- More aggressive cleanup customization
- Tray icon and quick controls (moved to v1.2)
