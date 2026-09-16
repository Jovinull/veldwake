# ADR-0002: Presentation-independent authority

- Status: Accepted
- Date: 2026-09-16
- Owners: Project owner
- Supersedes: None
- Superseded by: None

## Context

The game may support multiplayer later, while world generation, simulation, saves, testing, and internal tools need headless operation. Retrofitting authority after client code owns state would create severe architectural and security cost.

## Decision

Authoritative world/gameplay/simulation state lives outside client presentation. Rendering consumes views/snapshots and submits player intent. World generation never requires a GPU; server code never depends on presentation. Single-player respects a `client <-> local authoritative server` boundary, even when initially implemented in-process without transport.

## Alternatives considered

- Client-owned single-player first, multiplayer retrofit later: lower immediate ceremony but high coupling/rewrite risk.
- Network transport from day one: unnecessary implementation scope; authority boundaries can exist without transport.

## Positive consequences

- Headless tests/tools/server become natural.
- Multiplayer, replays, saves, and observability have a coherent source of truth.
- Render failures/settings cannot silently redefine gameplay state.

## Negative consequences

- Commands/views and synchronization boundaries add early design work.
- Local single-player may require explicit orchestration and copying/interpolation.

## Future implications

M1 must not put authoritative entities in renderer/platform crates. Networking remains deferred; future protocol and prediction details require separate ADRs.
