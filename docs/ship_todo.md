# SayWrite Shipping TODO

This is the short list to get SayWrite to the point where it can compete with tools like Wispr Flow on Linux: press a hotkey, dictate anywhere, and trust the result.

## P0

- Make the host daemon the single dictation control path.
- Keep the GTK app as a client: button presses, hotkey activations, and transcript updates must all flow through the same D-Bus path.
- Finish GlobalShortcuts correctness.
- Filter portal activations by the registered session and shortcut id.
- Refresh reported hotkey status after successful portal registration so diagnostics stay truthful.
- Replace the current `ibus write` probe with a proper verified IBus path or explicitly keep it as a best-effort fallback and document that choice.
- Add end-to-end tests for the host-driven flow: start, stop, transcript-ready, insertion-result.

## P1

- Make Preferences reactive when model size changes.
- Recompute installed state, button label, and subtitle immediately when the selected model size changes.
- Add progress UI for model downloads instead of a blind spinner/button label swap.
- Harden cloud transcription.
- Validate base URL shape.
- Surface HTTP status/body in a user-readable way.
- Add at least one test around multipart request construction.

## P2

- Package and verify the host companion on a clean user account.
- Confirm `scripts/install-host.sh` works with `systemd --user`, D-Bus activation, and a standard `PATH`.
- Verify the Flatpak app can explain degraded mode clearly when the host companion is absent.
- Expand the support matrix.
- Test Wayland with `wtype`.
- Test X11 with `xdotool`.
- Test IBus-focused environments.
- Test clipboard fallback when no typing backend exists.

## Release Gate

Do not call this shipped until these are true:

- `cargo test` passes
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- app button and global hotkey both drive the same host dictation flow
- dictation works in at least one real Wayland setup and one real X11 setup
- degraded mode messaging is clear when direct typing is unavailable
