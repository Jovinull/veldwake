# Codex skills inventory and policy

Audited: 2026-09-16  
Codex CLI: `0.154.0`

## Supported mechanism

Official current Codex documentation says a skill is a directory containing required `SKILL.md` plus optional `scripts/`, `references/`, `assets/`, and agent metadata. Repository-scoped skills are discovered from `.agents/skills` between the working directory and repository root. User skills are under `$HOME/.agents/skills`; bundled system skills are supplied by Codex. Invocation can be explicit (`$skill-name`) or implicit from the description.

Source: [OpenAI Codex Skills documentation](https://developers.openai.com/codex/skills/) (consulted 2026-09-16). Local `codex --help`, `codex features list`, and filesystem inventory were also checked.

## Skills visible in this bootstrap session

| Skill | Origin | Purpose / restriction |
|---|---|---|
| `openai-docs` | OpenAI system | Used for current Codex/Skills documentation; official OpenAI sources only. |
| `imagegen` | OpenAI system | Raster image generation/editing; not relevant to this documentation bootstrap. |
| `skill-creator` | OpenAI system | Creates/updates a skill; intentionally not invoked because no triggered workflow is yet justified. |
| `skill-installer` | OpenAI system | Installs curated/GitHub skills; no external skill was trusted or needed. |
| `plugin-creator` | OpenAI system | Plugin scaffolding; not relevant. |
| `plugin-management` | Curated plugin cache | Plugin discovery/connection management; not needed for local bootstrap. |

The host also contains a curated `openai-templates` plugin cache with artifact templates. Those are not project engineering skills and were not installed or used for Veldwake.

## Installed or created for Veldwake

None. This is deliberate:

- always-on repository law belongs in [`AGENTS.md`](../../AGENTS.md);
- subsystem knowledge belongs in canonical docs and ADRs;
- an unproven collection of “Rust/render/voxel” skills would duplicate policy, consume context, and introduce unreviewed instructions.

## When to create a repository skill

Create `.agents/skills/<name>/SKILL.md` only when a repeatable triggered workflow has inputs, outputs, validation, and boundaries that exceed a short documented command. Likely future candidates include ADR authoring/validation, deterministic fixture regeneration, visual-regression review, benchmark capture, worldgen inspection, or release/save-migration checks.

Any external skill requires source/revision, license/provenance, full content review (including scripts/references), purpose, permissions/network behavior, risks, and removal/update instructions recorded here. Never install by name alone.
