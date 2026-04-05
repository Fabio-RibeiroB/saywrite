# SayWrite Implementation Plan

## Goal

Rebuild SayWrite into a Rust-first Linux dictation product that feels fast, elegant, and trustworthy. The app should aim for: install, grant mic access, press hotkey, dictate into normal input fields, with clear degraded modes when host integration is unavailable. Python is legacy and must not remain on the runtime path.

## Non-Negotiables

- No Python in the runtime path.
- No UI-owned subprocess maze.
- No fake keyboard injection as the primary insertion strategy.
- Flatpak remains the user-facing app.
- Host integration is a first-class companion, not an afterthought.
- Architecture must be idiomatic Rust: explicit types, message boundaries, long-lived services, minimal hidden global state.

## Target Architecture

### 1. `saywrite-app`

Rust GTK4/libadwaita Flatpak UI.

Responsibilities:

- onboarding
- settings
- diagnostics
- transcript preview
- model install flow
- service and host status
- graceful degraded UX

### 2. `saywrite-core`

Shared crate for:

- config types
- cleanup pipeline
- shared enums and DTOs
- error types
- readiness/diagnostic models
- protocol constants

### 3. `saywrite-service`

Long-lived Rust speech daemon.

Responsibilities:

- audio capture
- VAD / utterance segmentation
- ASR orchestration
- cleanup pass
- transcript events
- warm runtime lifecycle
- diagnostics

### 4. `saywrite-host`

Host-side Rust companion.

Responsibilities:

- global hotkey
- IBus integration
- focused-field insertion
- compatibility detection
- fallback insertion paths
- host-side status reporting

## Implementation Sequence

1. Convert the repo into a Cargo workspace.
Create:
- `crates/saywrite-app`
- `crates/saywrite-core`
- `crates/saywrite-service`
- `crates/saywrite-host`

Move the existing Rust GTK code into `saywrite-app`.
Move cleanup/config/protocol-ready types into `saywrite-core`.

Deliverable:
- workspace builds
- current app still launches

2. Extract shared core cleanly.
Move these concerns into `saywrite-core`:
- `AppSettings`
- `ProviderMode`
- transcript cleanup rules
- runtime/readiness structs
- transcript result/status enums

Requirements:
- no GTK dependencies in `saywrite-core`
- strong typing, no stringly-typed modes
- paths stored as `PathBuf`
- errors modeled consistently

Deliverable:
- app compiles against `saywrite-core`
- no duplicated types across crates

3. Introduce a minimal internal protocol boundary.
Define a small unstable protocol used by app/service/host. Do not overdesign it.

Initial operations:
- `GetReadiness`
- `StartDictation`
- `StopDictation`
- `CancelDictation`
- `GetTranscriptState`
- `InsertText`
- `GetHostStatus`

Initial event types:
- `ListeningStarted`
- `ListeningStopped`
- `PartialTranscript`
- `FinalTranscript`
- `InsertionSucceeded`
- `InsertionFailed`
- `ReadinessChanged`

Deliverable:
- protocol types exist in `saywrite-core`
- app code stops depending on direct local dictation internals

4. Build `saywrite-service` as a fake-but-real daemon first.
Implement a long-lived Rust daemon process with:
- process lifecycle
- readiness/status reporting
- fake transcript generation
- start/stop/cancel state machine
- event emission

Do not start with real ASR.
Prove the boundary first.

Requirements:
- no Python
- app communicates with daemon, not direct module calls
- support repeated start/stop without process restarts

Deliverable:
- app can drive fake dictation through the service
- onboarding/diagnostics can show service readiness

5. Switch the app to a client-only role.
Refactor `saywrite-app` so it does not own dictation logic.
It should:

- call service APIs
- render readiness state
- render transcript state
- invoke host insertion APIs
- present fallback explanations cleanly

Requirements:
- remove remaining direct dictation orchestration from UI code
- keep UX polished while architecture becomes stricter

Deliverable:
- app is a pure client for service/host boundaries

6. Spike `saywrite-host` early.
This is a feasibility and risk-reduction milestone, not polish.

Implement:
- host daemon skeleton
- host readiness reporting
- global shortcut experiments
- IBus integration prototype
- fallback insertion prototype
- compatibility matrix notes

Priority order:
- IBus primary path
- portal/global shortcut support where available
- clipboard fallback
- accessibility fallback only as fallback

Deliverable:
- documented proof of what works on at least one supported Linux desktop
- clear list of unsupported/degraded environments

7. Replace socket assumptions with real service boundaries.
Move away from ad hoc local socket logic in the UI.
Adopt a proper IPC boundary between app, service, and host.
D-Bus is the preferred target.

Requirements:
- app can discover whether service/host are available
- failures are typed and user-presentable
- reconnect/restart behavior is handled cleanly

Deliverable:
- app talks to service/host through the chosen IPC boundary
- diagnostics show exact component availability

8. Add real local speech backend in `saywrite-service`.
Only after the daemon boundary is proven.

Implement:
- audio capture in Rust
- utterance lifecycle
- local whisper backend adapter
- cleanup pass after transcript finalization
- cancellation and interruption behavior
- warm model/runtime lifecycle

Short-term acceptable:
- managed external backend process behind a Rust service adapter

Long-term target:
- persistent warm backend with low startup overhead
- library integration only if justified by measurements

Deliverable:
- local dictation end-to-end through the service
- latency metrics captured

9. Add readiness-driven onboarding.
On first launch, the app should verify:
- microphone permission
- service reachable
- host companion present
- insertion path status
- model installed or missing
- shortcut available or degraded

The onboarding should not dump technical jargon immediately.
It should guide the user through what is missing in product language.

Deliverable:
- first-run path that leads to successful first dictation or a precise degraded explanation

10. Package the real product shape.
Ship:
- Flatpak for `saywrite-app`
- native companion packaging path for `saywrite-host`
- service packaging decision aligned with Flatpak/host boundary

Deliverable:
- documented install flow for supported environments
- explicit support matrix

## Technical Guidance

- Prefer long-lived daemons over per-request subprocesses.
- Prefer explicit state machines over scattered booleans.
- Prefer enums for readiness, mode, insertion capability, dictation phase.
- Keep cleanup deterministic and testable.
- Do not over-abstract backend support too early.
- Do not optimize for cloud mode first.
- Measure latency before deciding whether `whisper.cpp` CLI is unacceptable.
- Preserve a single coherent happy path in the product.

## Risks To De-Risk Early

- Global hotkey support across desktops/compositors
- IBus compatibility with common apps
- Flatpak-to-host coordination
- Model distribution/install UX
- Warm-start latency for local ASR
- Recovery when host integration is missing

## Definition of Done for the First Major Agent Pass

The agent should finish with:

- a Cargo workspace
- `saywrite-core` extracted
- `saywrite-service` daemon skeleton
- `saywrite-host` daemon skeleton
- current app moved to `saywrite-app`
- app talking to the service boundary instead of owning dictation logic
- repo docs updated to reflect the new architecture
- build passing

## Explicitly Out of Scope for This Pass

- perfect IBus implementation
- final hotkey solution across all desktops
- final whisper binding choice
- production packaging for every distro
- full UI redesign

## Success Criteria

After this pass, the repo should clearly be on the right rails:

- no Python runtime dependency
- no architecture centered on UI subprocess calls
- app/service/host split visible in code
- host integration treated as core product surface
- implementation path toward press hotkey, dictate anywhere materially de-risked
