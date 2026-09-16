# Code and review quality

Status: **Accepted baseline**.

Review correctness, architecture, maintainability, performance evidence, security, test strength, documentation consistency, and scope—in that order of material risk.

## Review questions

- Does the behavior satisfy the stated requirement without silently changing it?
- Does authority stay outside presentation and generation remain headless?
- Are errors, cancellation, bounds, invalid data, and partial failure handled explicitly?
- Is new complexity justified by a current use case?
- Does a dependency duplicate existing capability or add risky native/build/license surface?
- Are tests capable of failing for the defect they claim to prevent?
- Are hot-path claims measured on a representative fixture?
- Are public/persisted contracts versioned and migration/cache effects understood?
- Does documentation still describe reality?

## Anti-patterns

Reject lint suppression without rationale, weakened assertions, arbitrary defaults, giant “manager” abstractions, accidental global state, unbounded queues, blocking frame work, undocumented `unsafe`, public APIs designed only for hypothetical reuse, and cross-subsystem convenience dependencies that violate direction.

High standards do not mean maximal abstraction. Straightforward, measured, well-named code with a small surface is preferred.
