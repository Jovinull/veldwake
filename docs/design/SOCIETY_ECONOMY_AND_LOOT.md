# NPCs, factions, economy, quests, and loot

Status: **Proposed system direction**.

These systems exist to create understandable world changes and player decisions, not to simulate an economy for its own sake.

## Aggregate state

Settlements/regions may track population, professions/capabilities, resources, production/consumption, danger, faction influence/control, trade links, migration pressure, and notable events. Named NPCs and groups can persist as compact records at distance and materialize into detailed entities locally.

## Causal chain example

```text
local resource -> settlement profession -> trade demand -> route
threat blocks route -> shortage/prices/migration -> changed settlement/faction state
player intervention -> route reopens or alternative emerges -> visible recovery/change
```

The source's Ravenfall example illustrates intent, not required lore: a settlement arose near iron/forest resources, trade created routes and intermediate villages, attacks disrupted caravans, and player action or inaction changed prices, population, and growth.

## Generated quests

Candidate quests arise from real state: resource shortage, missing caravan, dangerous route, faction conflict, migration, damaged infrastructure, ecological threat, or historical discovery. A quest must explain stakes, permit meaningful outcomes, and reconcile with state when ignored or solved through unexpected means.

```text
economy + simulation + NPCs + factions + events -> opportunity/conflict
```

This formula is a design direction, not a claim that all quests need no authored rules. Templates, narrative grammar, validation, and pacing remain necessary.

## Factions and cultures

Faction influence and territory can shape security, law, trade, architecture, equipment, and relations. Culture is not a color swap: it provides visual, material, musical, architectural, and historical grammar. Politics grows only as far as it yields legible gameplay.

## Loot

Loot connects gameplay role, geometry/material, craft quality, culture, history, and prior ownership. Avoid disposable procedural spam and region boundaries that arbitrarily erase value. Item provenance is worthwhile only if it changes meaning, choices, collection, reputation, abilities, or connection to world events.
