# World editing and building

Status: **Accepted capability direction; scope TBD**.

Minecraft inspires the idea that the world is matter, not Veldwake's identity or primary loop. Players may eventually dig, cut, destroy, build, plant, create roads/camps, and found or support settlements while remaining in an action RPG.

## Design goals

- Edits enable routes, preparation, defense, expression, recovery, and visible consequence.
- Damage from encounters can alter places; inhabitants may repair, adapt, abandon, or rebuild according to simulation state.
- Structures have gameplay function and cultural fit, not only decorative block placement.
- Player marks remain bounded, versioned, saveable, streamable, and compatible with regeneration.

## Engineering questions

- Distinguish generated baseline from sparse edits and fully authored player structures.
- Define ownership/conflict rules for local and future multiplayer worlds.
- Prevent world edits from invalidating navigation, simulation, structures, or chunk caches without reconciliation.
- Budget mesh rebuilds, lighting, physics, persistence, undo/recovery, and network replication.
- Decide what NPC rebuilding may change without violating player trust.

The first voxel milestones need edit/read correctness, not a full building game or settlement founder system.
