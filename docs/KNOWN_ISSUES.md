# Known issues and limitations

Last updated: 2026-09-16

| ID | Status | Issue | Consequence / next action |
|---|---|---|---|
| KI-001 | Open | `Veldwake` is only a working title; preliminary transcript research is not legal clearance. | Perform formal INPI/USPTO/EUIPO, store, domain, and counsel review before public branding. |
| KI-002 | Open | The only executable is a Windows-first diagnostic renderer, not a playable game; other OSes/backends are unverified. | Expand support only when a milestone has a concrete target and validation host. |
| KI-003 | Open | Integrated GPU and shared memory make the audit host useful as a conservative dev target, not a complete hardware matrix. | Add discrete-GPU and lower-end CI/manual test coverage later. |
| KI-004 | Open | Vulkan loader reports an OBS hook layer older than the requested Vulkan API and a missing layer-manifest registry lookup. | D3D12 remains the primary Windows path; reassess only if Vulkan testing becomes required. |
| KI-005 | Open | No project license is selected. | Owner decision required before public distribution/contributions. |
| KI-006 | Open | M1 through M3A have one-host visual validation but no automated visual regression or multi-adapter coverage. | Add stable capture fixtures and broader hardware coverage when rendered game content begins to change. |
| KI-007 | Accepted | `cargo-deny` reports transitive duplicate pairs for `hashbrown` and `syn` in the current `wgpu` graph. | Keep duplicate visibility as a warning; reassess on dependency upgrades rather than forcing transitive versions. |
