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
