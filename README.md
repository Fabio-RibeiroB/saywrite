# SayWrite

SayWrite is a Linux-first dictation project aimed at the Wispr Flow experience without the setup pain.

The goal is not "another speech-to-text demo." The goal is:

- global dictation that feels system-wide
- easy install and updates through Flatpak
- attractive, low-friction desktop UX
- text cleanup that removes filler words and rough speech artifacts
- optional command mode for punctuation, symbols, and editing verbs
- support for both local and cloud transcription modes

## Product Direction

SayWrite should feel like a polished desktop product, not a terminal utility:

- one obvious toggle for dictation
- clear microphone state
- super-fast transcript preview
- post-processing that turns spoken language into usable written language
- system integration that works in normal Linux text fields

## Architecture Summary

This project uses a hybrid model because a pure sandboxed Flatpak cannot reliably inject text into arbitrary applications.

1. The Flatpak app provides the visible product:
   - onboarding
   - settings
   - microphone controls
   - transcript preview
   - cleanup options
   - local model management
   - cloud mode selection
2. A host-side integration service handles system-wide text entry:
   - preferred path: IBus engine for real text-field integration
   - fallback path: accessibility/clipboard insertion for unsupported cases
3. A speech pipeline processes audio into cleaned text:
   - streaming ASR backend
   - rewrite/cleanup layer
   - spoken command parser
4. A backend probe keeps setup inside the app:
   - detect GPU vendor
   - choose CUDA, Vulkan, or CPU automatically
   - show local model and cloud API readiness in-product

More detail lives in [docs/architecture.md](/home/fabio/Documents/GitHub/saywrite/docs/architecture.md) and [docs/roadmap.md](/home/fabio/Documents/GitHub/saywrite/docs/roadmap.md).

## Why This Project Is Worth Doing

The current Linux options tend to fail in one of three ways:

- good engine, bad UX
- good UX, weak system integration
- powerful setup, hostile onboarding

SayWrite can be differentiated by taking the opposite stance: opinionated defaults, polished UI, and system integration designed from day one.

## Local Run

The current scaffold includes a GTK4/libadwaita shell:

```bash
python3 -m saywrite
```

For local development dependencies on Ubuntu-like systems:

```bash
./scripts/bootstrap-dev.sh
```

To vendor and build the local `whisper.cpp` runtime for development:

```bash
./scripts/setup-whispercpp.sh
```

To download the default local model for development:

```bash
./scripts/download-local-model.sh
```

To run the host-side insertion helper prototype:

```bash
./scripts/run-host-helper.sh
```

## Repository Layout

- `saywrite/` application package
- `data/` desktop metadata and icons
- `flatpak/` Flatpak manifest
- `docs/` product and architecture notes

## Current Status

This is a scaffold for the product direction and desktop shell. It now includes:

- a persisted first-run product shell
- local versus cloud provider selection
- deterministic cleanup preview behavior
- backend diagnostics with automatic GPU-path selection
- focused-field delivery through a host helper that tries accessibility insertion before clipboard fallback
- a Flatpak manifest draft

The remaining major milestone is replacing the helper fallback path with deeper input-method integration so the Flatpak setup can graduate from prototype to product.
