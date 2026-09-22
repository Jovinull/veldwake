# ADR-0009: Two-level landmark visibility and an eagerly derived world plan

- Status: Accepted
- Date: 2026-09-21
- Owners: repository owner
- Supersedes: None
- Superseded by: None

## Context

M8 puts generated landmarks in the world so that a direction can be worth choosing. That requires answering a question the repository had never had to answer: **can this be seen from there?** Placement depends on the answer, and placement happens inside `veldwake-procedural`, a crate that must not know what a camera is — [ADR-0002](0002-presentation-independent-authority.md) forbids authoritative code from depending on presentation, and the whole value of a headless world generator is that it can be tested without a GPU.

The naive resolutions are both wrong. Putting the camera, the field of view and the fog model into the generator makes world generation depend on the renderer, and a change to the follow camera's pitch would then change where the rocks are. Leaving visibility to the client makes placement blind: the generator would scatter landmarks and the client would discover, at run time, that the forest hides them.

A second problem arrived with the first. The plan that places landmarks is **fallible** — a world may offer no site that satisfies the composition — and **expensive**: the first working derivation cost `3.2 s` in release, and the optimised one costs `248–272 ms`. `TerrainGenerator` was `Copy`, free of interior mutability, and its generating method is `generate(coord) -> Option<Chunk>`, where `None` already means "the region does not reach here". There is no channel in that signature for "the world could not be planned", and a lazily derived plan would hide both the failure and the cost behind whichever chunk happened to be asked for first.

## Decision

**Visibility is answered at two levels, and neither level may absorb the other.**

The **world proxy** lives in `veldwake-procedural`. It answers occlusion: given an observer column and an eye height that is a *composition control* rather than a camera, how much of a silhouette stands clear of the terrain, the water and the canopy, at what elevation angles, and whether it is backed by sky. It contains no field of view, no pixel, no aspect ratio, no camera, no renderer and no fog. Placement may use only this.

The **presentation oracle** lives in `apps/client`. It answers framing and legibility: the real follow camera built by the real controller, the real projection, the real fog table from the same `Lighting` the renderer uploads, and normalised device coordinates. It does no raycasting and therefore never decides visibility alone. It cannot move a landmark; it can only report what the real view of one is.

A landmark is visible when both agree, and a test asserts the direction that matters: the proxy may not call visible what the camera cannot frame.

**The landmark plan is derived when a world is constructed, never when a chunk is generated.** `TerrainGenerator::new` returns `Result<Self, WorldError>`, holds an `Arc<LandmarkPlan>`, and is no longer `Copy`. A world that cannot carry its composition fails to be built, with a typed error naming which stage failed and how many candidates it examined. Reuse is explicit: `TerrainGenerator::with_plan` shares an already derived plan, and the client's `WorldSelection::build` produces one world where it used to build three.

## Alternatives considered

- **One visibility model, inside the generator.** Rejected: it inverts [ADR-0002](0002-presentation-independent-authority.md) and makes the position of world content a function of the renderer's field of view. A camera change would move rocks, and the generator could no longer be tested headlessly without also testing a camera.
- **One visibility model, inside the client.** Rejected: placement would be blind, and the client would have to move world content to fix it — which is world editing, explicitly out of scope, and would put authoritative content behind presentation.
- **A `OnceLock` inside the generator, filled on first use.** Rejected by the implementation review. It preserves `Copy` and an infallible constructor by hiding a fallible, expensive operation behind the first chunk request, where there is no way to report the failure and no way to predict the cost. "Cheap to clone" would have become "cheap to clone, and one of the clones pays for a second of work".
- **A global cache keyed by world identity.** Rejected for the same reason plus a new one: it is process-global mutable state in a crate whose entire value is being a pure function. The measurement also removed the motive — `cargo nextest` runs one process per test, so a global cache would not have helped the suite at all.
- **Making the plan infallible by falling back to a default composition.** Rejected for the golden world: a landmark placed where the composition does not hold is worse than no landmark, and silently producing one would make the failure invisible. Diagnostic worlds do fall back — to `LandmarkPlan::bare`, which has an overlook and no landmarks — because a seed nobody composed has no composition to honour.

## Positive consequences

- `veldwake-procedural` still compiles and tests without a GPU, a window or a camera, and placement is still a pure function of the world identity.
- A camera change cannot move world content; it can only change what a test says about framing.
- A world that cannot be planned fails at construction, where the caller can see it, rather than producing chunks that are missing their landmarks.
- The cost is where it can be measured and reused deliberately, and it was: `3.2 s` to `248 ms`, and three world constructions in the client down to one.
- The two levels disagree loudly rather than quietly. The test that compares them already caught a real mistake: a projection with no raycasting had called the deliberately hidden landmark visible.

## Negative consequences

- `TerrainGenerator` is no longer `Copy`, which touched every holder — including `TerrainChunkSource`, which also lost it.
- Constructing a world costs `248–272 ms` in release and `1,108 ms` in debug, paid once per generator. The test suite pays it once per test process (KI-035).
- Two models of visibility exist and can drift apart. Only the comparison test prevents that, and it can only prove one direction.
- The world proxy's `observer_eye` is a composition control that stands for a viewer. It is honest about being an art control, but it is still a number in the generator that a person chose by looking at the game.

## Future implications

- Any future content whose placement depends on being seen — a settlement, a road, a vista — uses the same split, and should reuse `WorldOccluders` rather than growing a second occlusion model.
- If the follow camera's pitch or field of view changes, the presentation oracle's tests are what will fail first, and the composition controls are what to reconsider — not the camera.
- If the derivation's cost ever matters again, the levers are the candidate lattice, the level-and-dry disc and the number of tests that build a whole world. A cache is not one of them.
- A generator that composes landmarks for an arbitrary seed needs controls derived from the valley rather than named for it (KI-033). That is a separate decision and probably a separate milestone.
