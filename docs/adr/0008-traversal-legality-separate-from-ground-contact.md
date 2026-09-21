# ADR-0008: Traversal legality separate from ground contact

- Status: Accepted
- Date: 2026-09-20
- Owners: Veldwake maintainers
- Supersedes: None
- Superseded by: None

## Context

M7 had to let a body walk the whole generated region rather than a scanned clearing five and a half world units across, and the owner decided that water blocks traversal. Two questions that had always had one answer suddenly had two.

`veldwake-character`'s `GroundSampler` answers *where the visible solid surface is*. [`INVARIANTS.md`](../engineering/INVARIANTS.md) CHAR-002 makes that exactness a contract: it returns the top face of the topmost solid voxel, never a smoothed height, and inside a river it returns the **bed**, because a sole has to rest on the block a viewer can see. M5 and M6 depend on all of that.

"May a body walk here" is a different question, and the cheapest way to answer it would have been to make `surface` return `None` over water. That would have been a contact model quietly becoming a rules model: the feet, the pelvis and the leg IK would have lost the one answer they need, in exactly the columns where they need it.

A second collision appeared at the same time and from the same direction. `movement::accepts` judged a step against `CharacterState::base_height`, which `veldwake-character` documents as the **smoothed** height the pelvis follows — the feet snap, the pelvis lags, IK absorbs the difference. That is a presentation filter, and it was deciding an authoritative rule. The arithmetic is unforgiving: at `PELVIS_RISE_TAU = 0.12 s` and 120 Hz the filter closes 6.71% of its gap per tick, so a body climbing at gradient `g` at the walk speed carries about `0.42 g` world units of lag, and with `max_step_up` exactly one terrain voxel *any* lag turns a legal step illegal. The same step was legal standing still and illegal while walking. M6 never saw it because its arena is asserted exactly level and its headless fixtures stand on flat ground.

Finally, M7 needed an offline reachability audit over 640,000 columns that could say not only *whether* a step was refused but *why*, so that a barrier could be attributed to water, to a step-up, to a drop or to the edge of the region.

## Decision

**Ground contact and traversal legality are separate questions with separate types.** `GroundSampler` is unchanged and answers where the visible solid surface is, river bed included. `veldwake_combat::TraversalLegality` is a new two-method-free trait — one method, `walkable(x, z) -> bool` — that is a **veto and never a height**. It is consulted about a move's **destination only**, so a body that somehow stands somewhere forbidden can still walk out of it.

**Movement legality is judged between exact support surfaces.** `check_move` compares `ground.surface(from)` against `ground.surface(target)`; it never reads `base_height`. The pelvis filter remains, for pose, where it belongs. This is recorded as invariant **MOVE-001**.

**There is exactly one implementation of the rule, and it returns why.** `movement::check_move` yields `Result<(), MoveBlockReason>` with six causes tested in a documented order — `NonFinite`, `Arena`, `Traversal`, `MissingGround`, `StepUp`, `Drop`. `accepts` is a wrapper, `try_move` uses it, and the reachability audit **calls it** rather than reimplementing the conditions.

**"This column has water" is one predicate, in the crate that writes the voxels.** `TerrainSample::has_water_voxel()` is `water_surface_y() > surface_y()`, which is exactly when `fill_terrain` writes at least one water cell. The continuous relation `water_surface > height` keeps its own name, `is_submerged()`, and the two genuinely disagree on columns where both values floor to the same voxel and the viewer sees dry ground. This is invariant **TRAVERSE-001**.

The boundary: this covers one finite region, one traversal veto with one reason in it, and a body that walks. It is not a movement framework, a navigation system or a physics adapter.

## Alternatives considered

- **Make `GroundSampler::surface` return `None` over water.** Rejected. It is the smallest change and the worst one: the feet, the pelvis and the IK solver would lose the height they need precisely where a shoreline needs it, and CHAR-002 would become false. A contact model would have silently become a rules model.
- **Give `GroundSampler` a second method, `walkable`.** Rejected: it puts a gameplay rule in the crate that owns bodies, and every implementor — including the flat, ramp and stepped test grounds — would have to answer a question it has no opinion about.
- **Keep judging steps against `base_height` and tune the filter.** Rejected. Tuning cannot fix a rule whose answer depends on how long the body has been climbing; it can only move where it goes wrong.
- **Let the audit recompute the refusal conditions.** Rejected as the classic second implementation. The audit is the one consumer most likely to be written once and then left alone while the rule moves.
- **A `Walkability` trait that returns an `Option<f64>` height as well as a verdict.** Rejected: it re-merges the two questions in a new shape, and nothing needs a second source of height.
- **Water as a slow surface, or wading.** Not considered on the merits: the owner decided water blocks, and a milestone that gives water behaviour is where that gets reopened.

## Positive consequences

- CHAR-002 stays true, and every M5 contact claim stays valid: the M5 regression figures are byte-identical after M7.
- Movement legality became a pure function of two exact heights and a veto, so it is testable by construction — including the test that three bodies with the pelvis settled, half a unit behind and forty units behind walk identical trajectories.
- An audit of 640,000 columns can attribute every barrier to a cause using the game's own function, which is what makes "water `2,464`, step-up `44,495`, drop `1,273`" a statement about the game rather than about a model of it.
- Water means one thing in the generator, in traversal and in the invariant, and the place a viewer sees dry ground is the place a body may stand.
- M6 was unaffected: its arena is level and its fixtures are flat, so every locked signature is unchanged.

## Negative consequences

- `Encounter::step` takes a world rather than a sampler, which touched sixty-four call sites once.
- Two traits now describe the ground, and a reader has to know which answers which. The naming carries that and the module documentation states it; nothing enforces it.
- The veto is consulted per candidate move, so a legality implementation that is expensive is expensive per tick. The client's is a terrain field sample; the audit's is a grid lookup, and a test asserts the two agree.
- `MoveBlockReason`'s order is now contract rather than implementation detail, because an audit reports the first cause that applies.

## Future implications

- Anything that makes a column untraversable for a reason that is not water — a wall, a claimed plot, a hazard — adds a variant to the veto's implementation, not a method to `GroundSampler`.
- A traversal verb that crosses water (swimming, a boat, a bridge) changes what the veto answers and must bump `TRAVERSAL_RULE_VERSION`, which participates in the route signature precisely so that such a change cannot pass silently.
- A movement rule that needs more than two heights — a ledge grab, a climb — is the point to re-argue this, because it is the first thing `check_move`'s shape would not express.
- The reachability audit is the obvious first consumer of a headless server or a generation-time placement step. It lives in `apps/client` because it has one consumer today; a second is the trigger to extract it, and it was written so that extraction is a move rather than a rewrite.
