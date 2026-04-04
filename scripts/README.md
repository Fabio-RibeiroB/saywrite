# Dev Scripts

`bootstrap-dev.sh` installs local host dependencies needed to develop SayWrite on Ubuntu-like systems.

Use:

```bash
./scripts/bootstrap-dev.sh
```

This is for development only. It is not part of the intended end-user setup story.

`setup-whispercpp.sh` vendors and builds `whisper.cpp` for local development.

Use:

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

Use:

```bash
./scripts/download-local-model.sh
```

`run-host-helper.sh` starts the first host-side insertion helper.

Use:

```bash
./scripts/run-host-helper.sh
```
