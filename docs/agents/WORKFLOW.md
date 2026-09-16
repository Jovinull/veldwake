# Agent workflow

This workflow operationalizes [`AGENTS.md`](../../AGENTS.md).

## Start

1. Read the required entry documents from [`docs/INDEX.md`](../INDEX.md).
2. Inspect `git status`; preserve unrelated user/agent changes.
3. Search with `rg`/`rg --files` before creating names, systems, or documents.
4. State assumptions and classify work as accepted requirement, proposal, experiment, or research.
5. Identify applicable invariants, ADRs, tests, and documentation.

## Implement

- Work in the smallest subsystem surface possible.
- Keep canonical sources distinct from generated artifacts.
- Add dependencies only after a written point-of-need justification.
- Keep authority and GPU-independent logic testable headlessly.
- Record discoveries as they become durable, without creating a session log.

## Validate and hand off

Run applicable gates and report exact status. Review diffs for accidental scope, secrets, generated files, and documentation drift. Update `PROJECT_STATE.md` for material capability/tool changes and keep `HANDOFF.md` focused on the next continuation point, not chronology.

## Multi-agent coordination

Use separate worktrees/branches when supported, give agents bounded ownership, and avoid concurrent edits to canonical state documents without coordination. Integrating work requires checking dependency direction and reconciling documentation; passing tests in isolated branches does not prove the combined tree.
