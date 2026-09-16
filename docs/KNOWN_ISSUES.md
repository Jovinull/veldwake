# Known issues and limitations

Last updated: 2026-09-16

| ID | Status | Issue | Consequence / next action |
|---|---|---|---|
| KI-001 | Open | `Veldwake` is only a working title; preliminary transcript research is not legal clearance. | Perform formal INPI/USPTO/EUIPO, store, domain, and counsel review before public branding. |
| KI-002 | Open | There is no playable executable or renderer. | Expected during M0; proceed only to M1. |
| KI-003 | Open | Integrated GPU and shared memory make the audit host useful as a conservative dev target, not a complete hardware matrix. | Add discrete-GPU and lower-end CI/manual test coverage later. |
| KI-004 | Open | Vulkan loader reports an OBS hook layer older than the requested Vulkan API and a missing layer-manifest registry lookup. | D3D12 remains the primary Windows path; reassess only if Vulkan testing becomes required. |
| KI-005 | Open | No project license is selected. | Owner decision required before public distribution/contributions. |
| KI-006 | Open | GitHub CLI is authenticated with broad repository/organization-capable scopes on this user profile. | Keep credentials in the OS keyring, do not expose tokens, and require explicit owner authorization before remote/public/destructive operations. |
