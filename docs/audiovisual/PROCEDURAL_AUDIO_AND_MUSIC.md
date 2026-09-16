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

DSP candidates include filtered noise, envelopes, modal resonators, wavetable/FM/subtractive synthesis, physical modelling, convolution/algorithmic reverb, delay, and dynamic processing. `cpal` is a future low-level I/O candidate, not yet a dependency.

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
