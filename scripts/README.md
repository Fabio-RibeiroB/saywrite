# Scripts Guide

These scripts support local development and fallback desktop integration flows. The current supported architecture is the single native Rust GTK app.

## Current Development Scripts

`bootstrap-rust-dev.sh` installs Rust and GTK/libadwaita build dependencies on Ubuntu-like systems.

```bash
./scripts/bootstrap-rust-dev.sh
```

`bootstrap-dev.sh` installs host-side development dependencies on Ubuntu-like systems.

```bash
./scripts/bootstrap-dev.sh
```

`setup-whispercpp.sh` vendors and builds `whisper.cpp` for local development.

```bash
./scripts/setup-whispercpp.sh
```

Optional explicit modes:

```bash
./scripts/setup-whispercpp.sh vulkan
./scripts/setup-whispercpp.sh cuda
./scripts/setup-whispercpp.sh cpu
```

`download-local-model.sh` downloads the default local Whisper model into SayWrite's data directory.

```bash
./scripts/download-local-model.sh
```

## Shortcut Fallback Scripts

`install-gnome-shortcut.sh` installs a GNOME custom shortcut fallback that calls the host D-Bus toggle command. Use this if the XDG GlobalShortcuts portal does not work on your desktop.

```bash
./scripts/install-gnome-shortcut.sh
```

`run-global-dictation.sh` is the debounced helper command used by the GNOME shortcut fallback. It calls `ToggleDictation` on SayWrite's compatibility D-Bus interface with a guard against rapid repeated invocations.

```bash
./scripts/run-global-dictation.sh
```
