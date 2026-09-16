# World memory and simulation

Status: **Accepted architecture concept; simulation rules Proposed**.

World memory is the product's main expression of Consequence. It records enough durable causal state for settlements, prices, routes, threats, faction influence, ecology, and stories to respond visibly. It does not mean preserving every object or simulating every NPC in full detail forever.

## Simulation levels

```text
GLOBAL / DISTANT SIMULATION
aggregate, cheap, low-frequency
population | resources | trade | danger | faction control | migration | events

              materialize / reconcile

LOCAL SIMULATION
detailed, physical, high-frequency
entities | navigation | combat | physics | animation | local behavior
```

Distant state becomes local entities near relevant players. Local outcomes collapse back into aggregate state when detail is no longer needed. Materialization and reconciliation must be deterministic enough to avoid obvious discontinuities and must be inspectable.

## Initial model boundary

Begin with a small causal vocabulary: resources, population, danger, faction influence, and trade. Add ecology, migration, conflict, weather disasters, and richer politics only when a player-visible consequence and computational model are justified.

## Design rule

> Do not simulate something deeply if its consequences never reach the player.

Every persistent fact carries save/migration/debug costs. World state must expose causal provenance so tools can answer “why is this settlement declining?” instead of presenting opaque randomness.
