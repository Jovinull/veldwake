# Environment report

Audit date: 2026-09-16  
Method: non-destructive PowerShell/CIM, registry, `dxdiag`, `vulkaninfo`, version/help commands, and official installers. Values describe this host, not project requirements.

## System

| Item | Observed |
|---|---|
| OS | Microsoft Windows 11 Home Single Language, 64-bit, version 10.0.26200, build 26200 |
| Architecture | x86-64 |
| Hostname | `JOVINULL` (recorded locally; no security-sensitive domain data observed) |
| Shell | Windows PowerShell 5.1.26100.9444 |
| CPU | Intel Core i5-1335U, 10 physical cores / 12 logical processors |
| RAM | 16,384 MB installed; CIM reported 15.68 GiB physical |
| Project drive | `C:` — 457.62 GiB total, 167.55 GiB free at audit |
| Display | 1920x1080 at 60 Hz |

## Graphics

| Item | Observed |
|---|---|
| GPU | Intel Iris Xe Graphics, integrated |
| Driver | Intel `32.0.101.7088`, dated 2026-06-16 |
| Memory | `dxdiag`: 128 MiB dedicated + 8,027 MiB shared (8,155 MiB display total). Do not describe this as 8 GiB dedicated VRAM. |
| DirectX | DirectX 12; feature levels through 12_1; WDDM 3.2 |
| Vulkan runtime | Instance 1.4.313; device API 1.4.323; Intel proprietary driver 101.7088 |

Expected initial `wgpu` backend on Windows: **D3D12**. The device exposes a viable Vulkan runtime too, but no Vulkan SDK is needed for the planned D3D12 path. `vulkaninfo` warned about a failed layer-manifest registry lookup and an OBS hook layer advertising Vulkan 1.3; this is recorded in known issues, not treated as a D3D12 blocker.

Integrated shared memory and thermally constrained mobile CPU/GPU behavior make this a useful conservative development host. They do not replace a target-hardware matrix or discrete-GPU validation.

## Development tools

| Tool | Version/status |
|---|---|
| Git | 2.55.0.windows.3 |
| Git LFS | 3.7.1; present, not configured for this repository |
| GitHub CLI | 2.96.0; authenticated to `github.com` as `Jovinull` over HTTPS; no remote created |
| Codex CLI | 0.154.0 |
| rustup | installed during bootstrap; stable x86_64-pc-windows-msvc default |
| rustc | 1.98.1 (`48a229cea`, 2026-09-01), LLVM 22.1.8 |
| Cargo | 1.98.1 |
| rustfmt | 1.9.0-stable |
| Clippy | 0.1.98 |
| cargo-nextest | 0.9.144, installed with `cargo install --locked`; installer emitted warnings for two yanked transitive lockfile packages—see note below |
| Visual Studio Build Tools | 2022 17.14.39; complete/launchable; VC x86/x64 component present |
| MSVC toolset | 14.44.35207; `link.exe` present |
| Windows SDK | 10.0.26100.0 |
| LLVM command-line tools | clang/lld 22.1.8 |
| Ninja | 1.13.0 (from Python environment) |
| Python | 3.12.10 |
| CMake | not found on PATH |
| `dxc` / `fxc` | not found on PATH; not required for initial `wgpu` WGSL use |
| `sccache` | not installed; unnecessary at current workspace size |

### nextest supply-chain note

The locked `cargo-nextest 0.9.144` installation warned that its upstream lockfile selects yanked `chacha20 0.10.0` and `der 0.8.0`. Yanked does not itself prove a vulnerability, and this tool is not a shipped game dependency, but the warning is preserved. Reassess/upsize when a release eliminates these locked entries; do not ignore an advisory if one appears.

## Deliberately not installed

- Vulkan SDK: no current need; Vulkan runtime already comes from the driver.
- CMake: no current dependency requires it.
- `cargo-deny` / `cargo-audit`: configure when the game workspace has third-party dependencies so policies produce meaningful results.
- `cargo-llvm-cov` / `llvm-tools-preview`: configure when behavior tests can yield meaningful coverage.
- `cargo-bloat`, `sccache`, Tracy: defer until binary size/build latency/profiling justify them.
- `cargo-fuzz`: Windows/native workflow limitations and absence of parsers make it premature; reassess with a supported target/CI strategy.
- Criterion/proptest: add as development dependencies only with the first suitable benchmark/property.

## Installation record

The `winget` Rustup package had no applicable installer and made no change. Rustup was then downloaded from `https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe` and installed non-interactively with profile `default`, stable MSVC host, and `--no-modify-path`. Downloaded installer SHA-256: `6F4BEF66261261FCB43131BE8720BAB817D403A09EDEC7455C371974B90BDB7E`.

No administrator bypass, reboot, Vulkan SDK, remote repository, LFS tracking, or unrelated software was introduced.
