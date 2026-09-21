# Development setup

## Supported bootstrap host

The repository currently validates on Windows 11 x86-64 with Visual Studio 2022 Build Tools, Windows SDK, and the pinned Rust MSVC toolchain. Other platforms are not rejected, but are unverified until a milestone adds support and CI.

## Prerequisites

1. Git.
2. On Windows, Visual Studio Build Tools with **Desktop development with C++** / MSVC x64 and a current Windows SDK.
3. [rustup](https://rustup.rs/) from the official Rust project.

Do not install Vulkan SDK merely for the Windows `wgpu` path. M1 is expected to use D3D12 through `wgpu`.

## Rust and PATH

The repository's `rust-toolchain.toml` installs/selects exact Rust `1.98.1` with `rustfmt` and Clippy. Ensure Cargo's bin directory is available in the current PowerShell session:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
rustup show
```

Install nextest if missing:

```text
cargo install --locked cargo-nextest --version 0.9.144
```

CI and local setup intentionally use `cargo-nextest 0.9.144`, the current stable crates.io release verified on 2026-09-16. Record the actual installed version and warnings when changing build tools; update CI and this command together.

Install the dependency-policy tools at the same intentional versions as CI:

```text
cargo install --locked cargo-deny --version 0.20.2
cargo install --locked cargo-audit --version 0.22.2
```

## Validate

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features
cargo nextest run --workspace
cargo test --workspace --doc
cargo metadata --format-version 1 --no-deps
cargo deny check
cargo audit
```

M1 contains meaningful headless tests, so an empty nextest suite is a failure.

Run the Windows diagnostic renderer with:

```powershell
$env:RUST_LOG = "info"
cargo run -p veldwake-client
```

The client logs two aggregate `M3B streaming` lines every five seconds; there is no per-chunk logging. `F1` cycles the debug views (`off`, `lod`, `residency`, `boundaries`) and `F2` toggles their wireframe boxes. The interval line reports debug work and, when enabled, aggregate cache lookup/rejection/cleanup/write/byte/timing counters. In `off`, debug per-frame allocations/writes/draws are zero; fixed startup resources and pooled slots may remain allocated. `VELDWAKE_CACHE_DIR` opts into the experimental M3D cache and names its directory. Unset means no cache filesystem access. When set, temporary cleanup and footprint discovery run synchronously once during event-loop initialization; later per-chunk I/O runs on the streaming worker and never in the frame hot path. Open failure is logged and falls back to cacheless execution. Clearing is explicit: delete the directory or call `ChunkCache::clear`. `F3` toggles the weather between `clear` and `overcast`. `VELDWAKE_PROFILE` selects `default`, `m3c-baseline`, `m3c-banded`, `m4-golden`, or `m4-golden-banded`; unknown values warn and use default. `VELDWAKE_WORLD` selects `golden` (the M4 slice, the default), `diagnostic` (the M3 corridor), or `seed:<decimal or 0x hex>`; `VELDWAKE_POSE` names one of the golden camera poses (`valley-wide`, `river-bend`, `pond-shore`, `cliff-face`, `forest-pocket`, `depth-stack`) and is ignored for the diagnostic corridor. Unknown values for either warn and fall back.

`VELDWAKE_CHARACTER` selects what the M5 humanoid does, and is ignored when the world has no terrain to stand on:

| value | what it does |
|---|---|
| `off` | no character at all; this is what the M3 and M4 regression smokes use |
| `idle` (default) | the golden humanoid standing in the portrait clearing, breathing |
| `pose:<name>` | held at one named pose: `rest`, `idle-a`, `idle-b`, `walk-contact`, `walk-passing`, `walk-push`, `run-contact`, `run-flight` |
| `walk:<0..7>` / `run:<0..7>` | held at one of eight equally spaced phases of that gait, for the contact sheet |
| `course` | driven around `meadow-crossing`, the level diagnostic loop |
| `slope` | driven up and down `terrace-climb`, the terraced loop |
| `slope-stand` | standing on one terrace with the edge a third of a world unit in front of the toes |
| `sturdy` | the second fixture standing in the same place, for comparison |

`VELDWAKE_POSE` also accepts the ten character poses — `character-front`, `character-three-quarter`, `character-side`, `character-silhouette`, `character-detail`, `character-contact`, `character-slope`, `character-scale`, `character-in-scene`, `character-walk-by` — which are placed relative to wherever the selected character stands rather than at a fixed world coordinate. The client logs a `character ready` line at startup and a `character state` line every five seconds; both are described in [`../engineering/OBSERVABILITY.md`](../engineering/OBSERVABILITY.md).

`VELDWAKE_ENCOUNTER` runs the M6 combat slice or the M7 traversal session, and defaults to `off`, which is a regression contract rather than a default: with it unset the client behaves exactly as it did before M6, and in particular opens no audio device at all.

| value | what it does |
|---|---|
| `off` (default), `none` | no encounter, no second actor, no weapon, no effects, no audio device |
| `armed`, `play`, `playable` | playable. The encounter arms itself on the first input |
| `script`, `scripted` | the reference script drives the player, so a fight runs unattended |
| `moment:<name>` | replays the script to a named moment and freezes there |
| `moment:<name>+<ticks>` | the same, some whole ticks later, for a frame-exact motion strip |
| `traverse`, `traversal`, `walk` | the M7 traversal session: walk the region, meet the adversary, fight, carry on |

The moment names are `faceoff`, `telegraph-early`, `telegraph-late`, `player-anticipation`, `player-active`, `confirmed-hit`, `hit-reaction`, `adversary-active`, `successful-dodge`, `player-hit` and `defeat`; an offset beyond one second is refused as a typo. `VELDWAKE_CHARACTER` is ignored while an encounter runs, because the two combatants *are* the characters, and the client says so in the log rather than silently dropping it.

While an encounter is armed: `WASD` moves relative to the camera, `J` or the left mouse button attacks, `K` or `Space` dodges, `F4` detaches the camera to look around, and `F1` cycles to a `combat` debug view that draws the hurt volumes and blade endpoints a hit is decided by. `VELDWAKE_POSE` also accepts the five combat poses — `combat-side`, `combat-close`, `combat-shoulder`, `combat-wide`, `combat-plan` — which, for a frozen moment, are placed against the two bodies rather than against the arena, so every moment is framed the same way wherever the fight drifted to. The client logs an `encounter ready` line at startup, an `audio device open` line when a device is found, and `combat state` and `combat work` lines every five seconds; all are described in [`../engineering/OBSERVABILITY.md`](../engineering/OBSERVABILITY.md).

`traverse` is the M7 session rather than a fight in a clearing. The player starts at the named route's start and the adversary stands at its far end, dormant until the player comes inside its aggro radius; streaming demand follows the player's body rather than the camera, so `F4` detaches the view without moving the world; water blocks movement, so a body walking at a river stops at the waterline and slides along the shore; and the session continues past the outcome — a defeated adversary stays where it fell, and a defeated player returns to its configured start. The controls are the armed encounter's, except that a dodge is refused while the adversary is still dormant. The client logs a `traversal route ready` line and one `traversal checkpoint` line per named place at startup. See [`../planning/M7_TRAVERSABLE_REGION.md`](../planning/M7_TRAVERSABLE_REGION.md).

Headless evidence comes from `cargo run --release -p veldwake-streaming --bin streaming-probe`, which accepts profile or source names to run a subset, from `cargo run --release -p veldwake-procedural --bin terrain-probe`, from `cargo run --release -p veldwake-character --bin character-probe`, which takes `body`, `parts`, `skeleton`, `collision`, `palette`, `poses`, `contact`, `signature` and `bench` with an optional `--fixture golden|sturdy|varied`, and from `cargo run --release -p veldwake-combat --bin combat-probe`, which takes `weapon`, `spec`, `reach`, `moments`, `script`, `partition`, `signature`, `bench`, `trace`, `dodge`, `bodies`, `aim` and `contact [player|adversary] [golden|sandbox]`. None of the four needs a GPU, a window or an audio device.

The impact audio has one piece of evidence a test cannot produce. `cargo nextest run -p veldwake-client --run-ignored all the_listening_fixture` renders six deterministic seconds of every sound the fight makes to `%TEMP%eldwake-m6-combat-sounds.wav`, for a person to listen to. It is ignored by default and the file is not versioned.

If a command or graphical desktop is not available, report it as BLOCKED rather than substituting an unrecorded tool.

## Optional tools

Do not preinstall the entire future tool list. Add coverage, benchmarks, profiling, fuzzing, CMake, or shader utilities only when the relevant configuration or subsystem exists. Prefer official releases and `cargo install --locked`; preserve provenance and warnings in the environment report.
