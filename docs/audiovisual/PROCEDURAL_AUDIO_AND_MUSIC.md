# Procedural audio and music

Status: **Accepted production constraint; implementation Exploratory**.

Veldwake should not depend on the creator recording a large sound library or manually composing a large soundtrack. Sound still requires direction, testing, mixing, accessibility, and performance budgets.

## Sound effects and ambience

Research and prototype parameterized synthesis for:

- footsteps from gait, mass, footwear, surface, speed, and weather;
- impacts from materials, energy, contact shape, transient noise, resonators, and space;
- weapon resonance and movement;
- wind, rain, fire, water, vegetation, and environmental beds;
- creature vocalization and simple stylized synthesized speech/voices;
- spatialization, filtering, reverb, voice prioritization, and variation control.

DSP candidates include filtered noise, envelopes, modal resonators, wavetable/FM/subtractive synthesis, physical modelling, convolution/algorithmic reverb, delay, and dynamic processing.

## What exists

`cpal` is no longer a candidate: M6 added it, pinned at `=0.18.2` with no features enabled, audited before it was added, and [ADR-0007](../adr/0007-procedural-impact-audio-boundary.md) records the boundary it sits behind. What exists is deliberately two sounds and no system:

- **A hit** — a noise transient through a fast envelope, a low body carrying the struck body's weight, and a short metallic ring over it.
- **A whiff** — noise through a one-pole filter whose corner rises and falls as a blade passes, with no impact in it anywhere, so a miss can be heard and can never be heard as damage.

Both are generated: no samples, no assets, no files, and a few hundred bytes of arithmetic each. Eight voices, a soft-knee limiter, and a deterministic integer noise source so the same fight renders the same samples on every host.

What does **not** exist, and must not be assumed from the above: footsteps, ambience, weather, vocalisation, spatialisation, filtering by distance or occlusion, reverb, a mixer, voice prioritisation beyond "displace the oldest", music of any kind, and any asset pipeline. The list at the top of this document remains research, not a plan of record.

## Music

Random notes are not music. An algorithmic/adaptive system should work with authored-in-code musical structure:

- motifs and transformations;
- scales, harmony, voice leading, rhythm, cadence, and form;
- synthesized instrument identities;
- culture, biome, time, weather, danger, location, and recent-event state;
- adaptive layers and bounded deterministic variation;
- repetition management, transitions, silence, and mix priority.

## Quality risks and validation

Procedural audio can sound synthetic, fatiguing, noisy, or emotionally incoherent. Prototype with listening tests early, capture deterministic render fixtures where possible, monitor clipping/loudness/CPU/voices, and keep an explicit exception path if a narrow generated or licensed source becomes necessary for quality. That exception must not silently recreate a traditional asset pipeline.
