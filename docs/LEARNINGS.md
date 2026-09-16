# Reusable learnings

Last updated: 2026-09-16

- On the audited Windows host, PowerShell did not initially expose Cargo because Rust was absent; rustup was installed with `--no-modify-path`. New shells may need `%USERPROFILE%\.cargo\bin` added explicitly for the session or user environment.
- Intel Iris Xe reports only 128 MiB dedicated video memory but about 8 GiB shared graphics memory. Do not treat WMI's `AdapterRAM` value as reliable VRAM capacity for an integrated GPU.
- D3D12 feature level 12_1 and Vulkan 1.4 are both available. This does not create a need for the Vulkan SDK: the runtime/driver is distinct from developer SDK tooling.
- Codex CLI `0.154.0` discovers repository skills under `.agents/skills`, not a guessed legacy project path. Repository instructions that should always apply belong in `AGENTS.md`; create a skill only for a reusable triggered workflow.
- The source transcript contains research claims tied to its date and several community links. Preserve them as leads; re-verify time-sensitive facts before making product or dependency decisions.
