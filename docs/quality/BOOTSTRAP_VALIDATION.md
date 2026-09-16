# Bootstrap validation

Executed: 2026-09-16 on the host in [`ENVIRONMENT_REPORT.md`](../environment/ENVIRONMENT_REPORT.md).

| Check | Status | Evidence |
|---|---|---|
| Source preservation | PASS | Original and preserved copy are 78,656 bytes with identical SHA-256 `153912EC0E71DD21B2F54A00AE5CD6B3C941A16A29CACD0DFC2C1A4319490385`. |
| `cargo fmt --check` | PASS | Exit 0 on pinned Rust 1.98.1. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | Exit 0. |
| `cargo build --workspace --all-features` | PASS | Exit 0. |
| `cargo nextest run --workspace` | NOT YET APPLICABLE | Correctly exited 4 because M0 has zero tests. No artificial test was added to obtain green. |
| `cargo nextest run --workspace --no-tests=pass` | PASS | Exit 0, explicitly reporting zero tests. Temporary M0 gate only. |
| `cargo test --workspace --doc` | PASS | Exit 0; zero doctests. |
| `cargo metadata --format-version 1 --no-deps` | PASS | Exit 0; one dependency-free workspace package. |
| Coverage/property/fuzz/visual/benchmark gates | NOT YET APPLICABLE | No behavioral, parser, visual, or hot-path code exists. |
| Relative Markdown links | PASS | Local Markdown targets checked programmatically after documentation creation. |
| Empty documentation files | PASS | No zero-byte Markdown files found. |
| Source URL preservation | PASS | All 46 unique source URLs are represented in `REFERENCES.md`; time-sensitive claims remain historical/unverified. |

The nextest no-tests exception must be removed from repository instructions and CI as soon as the first real test is introduced.
