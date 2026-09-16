# Agent skills and tooling policy

Audited: 2026-09-16. Codex CLI `0.154.0`; Claude Code present on the same host, loading project instructions from `CLAUDE.md`.

This repository is worked on by more than one agent runtime. The policy below is shared; only the discovery paths differ.

## Supported mechanisms

| Concern | Codex CLI | Claude Code |
|---|---|---|
| Always-on project instructions | `AGENTS.md` between working directory and repository root | `CLAUDE.md` (plus `CLAUDE.local.md`, untracked) |
| Repository skills | `.agents/skills/<name>/SKILL.md` | `.claude/skills/<name>/SKILL.md` |
| User skills | `$HOME/.agents/skills` | `~/.claude/skills` |
| Invocation | explicit `$skill-name` or implicit from the description | explicit `/<name>` or implicit from the description |
| Other agent configuration | `.codex/config.toml` | `.claude/settings.json`, `.claude/agents/`, hooks |

A skill is a directory containing a required `SKILL.md` plus optional `scripts/`, `references/`, `assets/`, and metadata. The two runtimes accept the same layout, so one skill can be published to both paths if it is ever justified.

Sources: [OpenAI Codex Skills documentation](https://developers.openai.com/codex/skills/) and [Claude Code skills documentation](https://code.claude.com/docs/en/skills) (consulted 2026-09-16; the older `docs.claude.com/en/docs/claude-code/skills` path now returns `301` to the `code.claude.com` host). Local `codex --help`, `codex features list`, Claude Code's own project-instruction discovery, and a filesystem inventory were also checked.

`AGENTS.md` remains the single canonical constitution. [`/CLAUDE.md`](../../CLAUDE.md) is a thin pointer to it plus runtime-specific notes; repository law is never duplicated or forked between the two files.

## Skills visible in the bootstrap Codex session

| Skill | Origin | Purpose / restriction |
|---|---|---|
| `openai-docs` | OpenAI system | Used for current Codex/Skills documentation; official OpenAI sources only. |
| `imagegen` | OpenAI system | Raster image generation/editing; not relevant to this documentation bootstrap. |
| `skill-creator` | OpenAI system | Creates/updates a skill; intentionally not invoked because no triggered workflow is yet justified. |
| `skill-installer` | OpenAI system | Installs curated/GitHub skills; no external skill was trusted or needed. |
| `plugin-creator` | OpenAI system | Plugin scaffolding; not relevant. |
| `plugin-management` | Curated plugin cache | Plugin discovery/connection management; not needed for local bootstrap. |

The host also contains a curated `openai-templates` plugin cache with artifact templates, and Claude Code sessions expose bundled system skills plus user-level skills from `~/.claude`. None of those are Veldwake engineering skills; none were installed or used for this repository.

## Installed or created for Veldwake

None, in either runtime. `.agents/skills` and `.claude/skills` do not exist. This is deliberate:

- always-on repository law belongs in [`AGENTS.md`](../../AGENTS.md);
- subsystem knowledge belongs in canonical docs and ADRs;
- an unproven collection of “Rust/render/voxel” skills would duplicate policy, consume context, and introduce unreviewed instructions;
- a skill that exists in only one runtime silently splits agent behavior.

## When to create a repository skill

Create `.claude/skills/<name>/SKILL.md` and `.agents/skills/<name>/SKILL.md` only when a repeatable triggered workflow has inputs, outputs, validation, and boundaries that exceed a short documented command. Publish it to both paths in the same change, or record why one runtime is excluded. Likely future candidates include ADR authoring/validation, deterministic fixture regeneration, visual-regression review, benchmark capture, worldgen inspection, or release/save-migration checks.

The same threshold governs Claude Code subagents, hooks, and committed settings: each is unreviewed instruction surface until it has a demonstrated repeatable use and is recorded here.

Any external skill, plugin, or MCP server requires source/revision, license/provenance, full content review (including scripts/references), purpose, permissions/network behavior, risks, and removal/update instructions recorded here. Never install by name alone.
