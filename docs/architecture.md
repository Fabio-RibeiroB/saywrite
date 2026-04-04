# Architecture

## Core Constraint

If the product must work in "basically any text input in Linux," the integration point matters more than the speech model.

A sandboxed Flatpak can capture audio and present UI, but it cannot be trusted to type into every host application by itself. For credible system-wide text entry, SayWrite should treat IBus as the primary insertion path.

## Proposed Components

### 1. Desktop App

The Flatpak application is the user-facing product.

Responsibilities:

- first-run setup
- microphone permissions and device selection
- hotkey configuration
- transcript preview
- cleanup settings
- model download and switching
- status display and diagnostics

Suggested stack:

- Python
- GTK4
- libadwaita

Rationale:

- fast iteration
- native Linux visual language
- good fit for Flatpak packaging

### 2. Speech Service

This service owns the dictation pipeline.

Responsibilities:

- audio capture
- VAD
- streaming transcription
- punctuation
- cleanup rewrite pass
- command parsing

Internal pipeline:

1. capture audio frames
2. run VAD to segment speech
3. stream to ASR backend
4. normalize transcript
5. apply cleanup rules
6. emit either text or editor commands

Possible backends:

- local `whisper.cpp` runtime for privacy and cross-vendor GPU support
- faster-whisper style worker
- remote provider as optional premium-like mode later

### 3. Host Integration Service

This service is the hard requirement for system-wide usability.

Primary strategy:

- implement an IBus engine
- commit cleaned text through the input-method framework

Why IBus:

- closer to normal text entry than fake key injection
- works with many GTK, Qt, and browser text inputs
- avoids brittle per-app hacks as the main strategy

Fallback strategies:

- clipboard paste insertion
- accessibility automation
- explicit "copy to clipboard" emergency mode

Fallbacks should exist, but the project should not be architected around them.

## Process Model

The product should separate concerns over D-Bus:

- Flatpak UI app
- host integration service
- speech worker

Suggested boundary:

- `io.github.fabio.SayWrite.App`
- `io.github.fabio.SayWrite.Service`
- `io.github.fabio.SayWrite.IBus`

Benefits:

- UI can restart without killing dictation
- host integration can run with tighter permissions than the UI
- backend swaps become easier

## Text Cleanup Strategy

This is a product differentiator, not a side feature.

Cleanup stages:

1. filler removal
2. hesitation cleanup
3. spoken punctuation conversion
4. capitalization normalization
5. whitespace cleanup
6. command extraction

Examples:

- "um can you send me the notes question mark" -> "Can you send me the notes?"
- "open bracket super alt close bracket" -> literal symbol sequence or configured macro
- "new paragraph" -> paragraph break

Important rule:

The cleanup layer must preserve intent. Users should be able to disable aggressive rewriting and preview raw transcript when needed.

## Hotkey and Activation

The UX target is hold-to-talk plus optional continuous mode.

Preferred modes:

- press and hold global shortcut
- double-tap modifier for quick dictation
- click tray toggle

Flatpak note:

The clean Flatpak-first activation path is the GlobalShortcuts portal where the host desktop supports it. This allows the app to register shortcuts through the portal rather than through ad hoc compositor-specific hacks.

Where portal support is insufficient, the app should degrade clearly and explain the limit in-product.

## Privacy and Model Strategy

Default product position:

- local-first
- offline-capable after model download
- explicit control over retention

Short-term:

- one dependable local runtime
- one balanced model size
- one fast mode for lower-end hardware
- one clear cloud mode for users who prefer lighter local requirements

Do not start by supporting every backend. Start with one path that feels coherent.

## Local Acceleration Policy

The product should auto-select the best local acceleration path that the machine can support:

- NVIDIA: CUDA
- AMD: Vulkan
- Intel: Vulkan where available
- otherwise: CPU

For Linux, `whisper.cpp` is the most coherent first local backend because it can target both CUDA and Vulkan from the same product architecture.

## Flatpak Position

Flatpak remains useful for the visible app even if host integration needs special handling.

The product can ship as:

1. Flatpak desktop app
2. small host companion package or helper

That is less ideologically pure than "Flatpak only," but much more likely to actually work.

## Non-Goals For V1

- editing arbitrary rich text semantics
- perfect support for every Wayland compositor
- language model style rewriting beyond conservative cleanup
- endless backend abstraction
