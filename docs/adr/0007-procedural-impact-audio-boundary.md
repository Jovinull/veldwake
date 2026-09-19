# ADR-0007: Procedural impact audio and its device boundary

- Status: Accepted
- Date: 2026-09-18
- Owners: Veldwake maintainers
- Supersedes: None
- Superseded by: None

## Context

M6's accepted scope includes procedural impact audio, and the milestone's own quality checklist includes looking for audio that arrives late or fires on the wrong event. Both require sound leaving a speaker while a person plays, so an offline render inspected afterwards cannot satisfy them.

The repository has no audio of any kind. [`PROCEDURAL_AUDIO_AND_MUSIC.md`](../audiovisual/PROCEDURAL_AUDIO_AND_MUSIC.md) records the production constraint — the game must not depend on a recorded sound library — and names `cpal` as "a future low-level I/O candidate, not yet a dependency". `AGENTS.md` requires a written point-of-need justification for any dependency and forbids speculative ones. `unsafe` is forbidden by workspace lint, so a hand-written platform backend is not available. CI runs on `windows-latest`, which cannot be assumed to have an audio endpoint.

[ADR-0003](0003-procedural-first-audiovisual-production.md) already decided that audiovisual content is produced by systems rather than authored, and `DETERMINISM.md` already states that identical audio sample output across backends is **not** promised.

## Decision

**Impact sound is synthesized from combat events by code, and the synthesis is a pure headless function.** A `CombatEvent` maps to typed `VoiceParams` (a transient band, modal resonator frequencies and decays, an envelope, a level and a pan), and a `render` function turns a fixed set of voices into samples. Neither step names a device, a stream, a thread or a backend, and both are covered by ordinary headless tests: finite output, no clipping, envelope and decay bounds, spectral band, whiff and hit distinguishable by parameter, voice limits, and deterministic parameters from an event.

**The device is a thin adapter in `apps/client`, and `cpal` is the one dependency it adds.** The point of need is concrete: the milestone requires real-time output, there is no safe path to a speaker without a backend, and `unsafe` is forbidden. Only the default platform backend is enabled; no ASIO, JACK or other optional backend is turned on.

**No audio crate is created.** There is one consumer, no build-isolation need and no dependency inversion, which is the same reasoning that kept the M3D disk cache inside `veldwake-streaming`. Audio is presentation, so it belongs on the client side of ADR-0002 rather than in a domain crate. The trigger to split it out is a second consumer — a headless audio-render tool, or a server — or music.

**The real-time callback allocates nothing, logs nothing, locks nothing, panics nothing and touches no renderer, world or combat state.** It reads voice requests from a bounded channel drained into a fixed voice array, and synthesizes into the output buffer. A full channel drops the request and counts it.

**Audio is opt-in with the encounter and never fatal.** No device is opened unless the encounter is active. A missing or failing device warns once, counts, and the game continues silent.

The boundary: this is one impact family plus a whiff and a telegraph cue. It is not an audio engine. There is no mixer graph, no reverb, no HRTF, no music, no streaming, no asset format, no DSP node system, and no spatialization beyond distance attenuation and a constant-power pan.

## Alternatives considered

- **No runtime audio; deterministic offline WAV fixtures only.** Rejected: it cannot answer whether audio arrives with the hit, which is an explicit acceptance question of the milestone. Offline WAV fixtures are still produced, as temporary artefacts for a listening check, but they are not a substitute.
- **A hand-written WASAPI backend.** Rejected: it requires `unsafe`, which the workspace forbids by lint and which `RUST_GUIDELINES.md` gates behind measured need, isolation, invariants and an ADR. Reimplementing a maintained backend to avoid a dependency is not a smaller surface.
- **A larger audio framework (`rodio`, `kira`, `fmod`-style).** Rejected: they bring mixers, decoders and asset formats for a project that has no audio assets and needs one synthesized family of sounds. The dependency surface would exceed the capability by a wide margin.
- **A separate `veldwake-audio` crate.** Rejected now, by the crate test: one consumer, no isolation need, no inversion. Named triggers to revisit are written into the decision.
- **Putting the synthesis in `veldwake-combat`.** Rejected: combat is authority, audio is presentation, and ARCH-003 forbids the mixing. Combat emits events; what they sound like is not its business.
- **Sample-accurate voice start offsets.** Rejected for this slice: voices start on a buffer boundary. The buffer is kept small enough that the resulting latency is measured and reported rather than argued about, and a sample-offset scheduler is machinery the slice does not need.

## Positive consequences

- Everything about the sound that can be checked without ears is checked without ears, and without a device, which means it runs in CI.
- A missing device degrades to silence rather than to a failed run, so the M3, M4 and M5 regression smokes and CI are unaffected by audio.
- The dependency is one crate with its default backend, auditable by `cargo tree`, `cargo deny` and `cargo audit`, and it enters with a written reason.
- Because parameters are derived deterministically from events, a repeated hit is not an identical sound, which is the fatigue risk R-004 names.

## Negative consequences

- Whether the sound is *good* is not decidable by an agent. The milestone records measured properties and an explicit owner listening check; it does not claim a perceptual judgement it cannot make.
- `cpal` is a real-time audio dependency with a platform backend per target, and a second target platform will re-open questions this slice does not answer.
- Voices starting on buffer boundaries means latency is quantized to the buffer. The figure is measured and stated rather than hidden.
- A bounded request channel can drop a request under a burst. The drop is counted and visible, and no sound is a better failure than a stall in a real-time callback.

## Future implications

- Music, ambience, footsteps and weapon resonance are all out of scope here and each needs its own evidence. A second family of sounds is the point to re-examine whether the fixed voice array and the single synthesis function still fit.
- A second consumer or a headless audio render tool is the trigger to extract a crate; the synthesis is already written so that the extraction is a move rather than a rewrite.
- Determinism of sample output is explicitly not promised and must not become an implicit contract. Voice *parameters* are deterministic and testable; samples are presentation.
