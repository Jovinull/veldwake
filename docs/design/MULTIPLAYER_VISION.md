# Multiplayer vision

Status: **Accepted architectural constraint; implementation deferred**.

Single-player and multiplayer share an authoritative gameplay boundary:

```text
single-player: client <-> local authoritative server
multiplayer:   client <-> remote authoritative server
```

This does not require network transport now. It requires gameplay authority, simulation, world generation, persistence, and protocol-shaped commands to live outside client presentation. The renderer consumes snapshots/views and submits intent; it does not create authoritative state.

## Future concerns

- latency tolerance, prediction, reconciliation, interest management, and anti-cheat;
- deterministic interfaces where replay/reconciliation needs them, without promising universal bitwise determinism;
- co-op ownership of world changes and conflict resolution;
- server resource budgets and headless operation;
- versioned protocol and compatibility policy;
- mod compatibility and capability limits.

QUIC through `quinn` is an exploratory candidate, not a current dependency or protocol decision. Large-scale MMO operation is a non-goal for the initial product.
