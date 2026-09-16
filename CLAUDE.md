# Claude Code entry point

This file exists because Claude Code discovers `CLAUDE.md`, not `AGENTS.md`. It adds nothing new: it routes Claude to the canonical rules and records the few facts that are specific to this agent runtime.

## Read before changing anything

The repository constitution is [`AGENTS.md`](AGENTS.md). It is binding for every agent, including Claude Code. Read it first, then the entry sequence in [`docs/INDEX.md`](docs/INDEX.md):

1. [`AGENTS.md`](AGENTS.md)
2. [`docs/PROJECT_STATE.md`](docs/PROJECT_STATE.md)
3. [`docs/INDEX.md`](docs/INDEX.md)
4. the document for the subsystem being changed
5. relevant [ADRs](docs/adr/README.md)
6. [`docs/agents/HANDOFF.md`](docs/agents/HANDOFF.md)

Definition of done, gate vocabulary (PASS / FAIL / BLOCKED / NOT YET APPLICABLE), invariants, and scope guardrails all live in those documents. Do not restate or relax them here.

## Documentation is part of every change

`AGENTS.md` makes this law; this section only makes it impossible to miss. A change is not done when the code works. It is done when the next agent, in either runtime, can read what exists, why it is that way, what was measured, and what remains, without rediscovering any of it. Update these in the same change, never in a follow-up:

- `docs/planning/M<n>_*.md` — the milestone document: status line, implementation result, observed evidence with real numbers, accepted behavior, remaining risks, and explicit non-goals.
- `docs/PROJECT_STATE.md` — stage, what works, what does not exist yet, active milestone.
- `docs/agents/HANDOFF.md` — the single next continuation point and the immediate risks; not a diary.
- `docs/LEARNINGS.md` — every reusable discovery below ADR scope: a bug and its cause, a refuted assumption, a test technique, a tooling quirk.
- `docs/KNOWN_ISSUES.md` — new defects and limitations, including work the agent could not verify itself.
- `docs/engineering/ARCHITECTURE.md`, `PERFORMANCE.md`, `TESTING_STRATEGY.md` — when structure, measurements, or test counts change.
- `docs/planning/ROADMAP.md`, `README.md`, `docs/audiovisual/RENDERING_VISION.md` — when milestone status changes.
- `docs/adr/` — only for a durable structural decision whose alternatives must outlive the milestone.

Record decisions and their reasons, not only outcomes. Record what was deliberately not done and why. Record numbers as observations on a named host, never as targets. Report every gate with the `AGENTS.md` vocabulary and say plainly what the agent could not verify (for example, an interactive smoke that needs a person at the screen).

## Agent runtime differences

- Project instructions: Claude Code loads `CLAUDE.md`; Codex CLI loads `AGENTS.md`. This file is the bridge, so always-on repository law stays in `AGENTS.md` and is never forked between the two.
- Skills: Claude Code discovers `.claude/skills/<name>/SKILL.md`; Codex CLI discovers `.agents/skills/<name>/SKILL.md`. Neither directory exists yet, by the policy in [`docs/agents/SKILLS.md`](docs/agents/SKILLS.md).
- Subagents, hooks, and settings (`.claude/agents/`, `.claude/settings.json`) are unused. Adding any of them is a repository change with the same review and documentation obligations as code.

## Host and shell

The validated host is Windows 11 x86-64 (see [`docs/environment/ENVIRONMENT_REPORT.md`](docs/environment/ENVIRONMENT_REPORT.md)). PowerShell is the primary shell; a POSIX shell is also available and each takes its own syntax. Cargo may not be on `PATH` in a fresh shell:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
```

Gate commands are in [`docs/environment/SETUP.md`](docs/environment/SETUP.md) and `AGENTS.md`. Report what actually ran; a gate skipped for a missing tool is BLOCKED, never PASS.

## Commit convention

- Conventional Commits, in English, subject line only.
- No body, no bullet list, no explanatory prose in the message.
- No trailers: no `Co-Authored-By`, no generator attribution.
- Author and committer are the repository's configured Git identity. Never pass `--author` or otherwise override it.
- Commit or push only when asked. Milestone work happens on its `feat/*` branch, not on `main`.

```text
feat: implement M3A multi-chunk correctness
docs: plan M3A multi-chunk correctness
```

Reasoning, evidence, and measurements belong in the milestone document, `PROJECT_STATE.md`, `HANDOFF.md`, `LEARNINGS.md`, or the pull request — not in the commit message.

## Pull request convention

Match PRs [#1](https://github.com/Jovinull/veldwake/pull/1), [#2](https://github.com/Jovinull/veldwake/pull/2), and [#3](https://github.com/Jovinull/veldwake/pull/3). Title is the milestone's Conventional Commit subject (`feat: complete M3A multi-chunk correctness`). Body is three sections:

- `## Summary` — lowercase bullets naming the capabilities and contracts the branch adds.
- `## Validation` — every gate command actually run, with the nextest count and the Windows/D3D12 smoke evidence when presentation changed. Use the `AGENTS.md` gate vocabulary; never list a command that was not executed.
- `## Scope` or `## Explicit non-goals` — what the branch deliberately does not introduce.

No AI attribution, generator footer, `Co-Authored-By`, or `🤖` marker in the title, body, or commits.
