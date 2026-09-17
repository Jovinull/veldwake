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

## Editing source from a shell agent

Large or repeated edits to Rust files on this host are safest as a small Python patch script written to the scratchpad with a file tool and then executed, rather than as inline shell heredocs. Multi-line Rust inside a Bash heredoc has failed here in ways that look like the patch worked: a vertical-tab escape swallowed part of a Windows path, a backslash line continuation inside a Rust string did not survive the round trip, and one heredoc aborted with a quoting error after a previous command in the same invocation had already written files.

- Make every replacement assert that its search string occurs exactly once, and fail loudly instead of writing a partial patch.
- When an exact-match replacement fails, inspect the real bytes (`sed -n 'N,Mp' file | cat -A`) before guessing. `rustfmt` line wrapping and string continuations routinely make the file differ from what you remember writing.
- Run `cargo fmt --all` after scripted edits and let the formatter own layout; do not hand-align generated code.
- Never run a patch script twice without checking whether the first run already applied part of it.

## Gate discipline

- Retries are for the host linker lock only (`LNK1104`, KI-008): a bounded loop of at most five attempts that retries **only** when the output contains that error. A failing assertion, panic, test, lint, or build error is evidence and is never re-run until it passes.
- A gate a tool could not run is `BLOCKED`, never `PASS`.
- Run `cargo doc` with `RUSTDOCFLAGS="-D warnings"`; a private intra-doc link added months earlier only fails once someone runs the gate, and it is cheaper to catch it in the change that touched the file.
- Check relative Markdown links and stray carriage returns before committing; both are trivially scriptable and both have bitten this repository.

## Validate and hand off

Run applicable gates and report exact status. Review diffs for accidental scope, secrets, generated files, and documentation drift. Update `PROJECT_STATE.md` for material capability/tool changes and keep `HANDOFF.md` focused on the next continuation point, not chronology.

## Multi-agent coordination

Use separate worktrees/branches when supported, give agents bounded ownership, and avoid concurrent edits to canonical state documents without coordination. Integrating work requires checking dependency direction and reconciling documentation; passing tests in isolated branches does not prove the combined tree.
