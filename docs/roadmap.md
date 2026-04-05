# Roadmap

## V0: Product Skeleton

- GTK shell with polished onboarding-style UI
- architecture docs
- Flatpak manifest draft
- local developer entrypoint

## V1: Reliable Dictation Core

- microphone selection
- press-to-dictate state machine
- mock transcript stream for UX development
- cleanup pipeline with deterministic rules
- transcript preview and accept/reject controls
- local and cloud provider selection in-product
- remove Python as a runtime dependency of the Rust app

Success criteria:

- user can speak into the app and see cleaned text appear in preview consistently

## V2: Real Speech Backend

- wire in a local streaming ASR backend
- tune latency budget
- add model download/install flow
- expose confidence and raw transcript debugging
- move from CLI-oriented orchestration toward a long-lived Rust speech service

Success criteria:

- local dictation feels responsive enough for normal messaging and note taking

## V3: System-Wide Insertion

- host-side IBus engine
- D-Bus bridge between app and integration service
- portal-based global hotkey registration where supported
- fallback clipboard insertion mode
- replace ad hoc host helper assumptions with a first-class Rust host service

Success criteria:

- cleaned text can be committed into common Linux text fields without manual copy/paste

## V4: Wispr-Like Features

- spoken commands for punctuation and symbols
- filler word cleanup controls
- application-aware formatting profiles
- tray icon and quick controls

Success criteria:

- product feels opinionated and pleasant, not merely functional
