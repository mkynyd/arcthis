# Repository Index

This map lists important maintained files and their current responsibilities. Generated output, local indexes, and fixture internals are intentionally omitted.

## Root

- `Cargo.toml` — Package metadata, restrained dependencies, release profile, and strict lint policy.
- `Cargo.lock` — Reproducible dependency resolution for builds and CI.
- `rust-toolchain.toml` — Pinned Rust 1.98.0 toolchain with Clippy and rustfmt.
- `README.md` / `README.zh-CN.md` — English and Simplified Chinese project introductions.
- `START.md` / `START.zh-CN.md` — Detailed, implementation-backed user guides.
- `ROADMAP.md` — Staged future format and capability plan.
- `AGENTS.md` — Long-lived Simplified Chinese engineering rules for coding agents.
- `CONTRIBUTING.md` — Contributor workflow and required quality checks.
- `LICENSE` — MIT license text.
- `.github/workflows/ci.yml` — macOS/Linux formatting, lint, test, and release-build workflow.
- `.gitignore` — Excludes build output, CodeGraph data, temporary archives, and local `log.md`.

## Library and CLI

- `src/lib.rs` — Public library exports and machine schema version.
- `src/main.rs` — Thin binary entry point and process exit handoff.
- `src/cli.rs` — Clap grammar, command dispatch, BrokenPipe handling, JSON errors, and exit-code mapping.
- `src/model.rs` — Shared serialized archive, entry, capability, inspection, copy, and verification models.
- `src/error.rs` — Typed library errors and stable public error categories.
- `src/output.rs` — Human renderers and typed schema-versioned JSON envelopes.

## Archive access

- `src/archive/mod.rs` — Deep format-independent `Archive` interface used by all commands.
- `src/archive/locator.rs` — Filesystem archive source abstraction prepared for future locator types.
- `src/archive/detect.rs` — Content-first ZIP, TAR, and TAR.GZ detection and header validation.
- `src/archive/backend/mod.rs` — Internal backend seam and entry-path rendering rules.
- `src/archive/backend/zip.rs` — ZIP metadata, streaming read/extract, and CRC verification adapter.
- `src/archive/backend/tar.rs` — TAR/TAR.GZ sequential metadata, streaming read/extract, and verification adapter.

## Materialization and lifecycle

- `src/security.rs` — Extraction path sanitizer and default resource-limit policy.
- `src/extract.rs` — Extraction planning, intelligent destinations, staging writers, limits, and commit.
- `src/pack.rs` — Source scan, ZIP/TAR/TAR.GZ creation, reopen verification, and no-clobber commit.

## Tests

- `tests/cli_access.rs` — Detection, list/tree/stat/inspect/read, JSON errors, and BrokenPipe integration tests.
- `tests/cli_extract.rs` — Destination rules, single/full extraction, collision, traversal, link, duplicate, warning, and resource-limit regressions.
- `tests/cli_pack_verify.rs` — Three-format pack/verify/extract round trips, Unicode/empty entries, collision, symlink, and CRC corruption tests.

## Design documents

- `docs/PRODUCT.md` — Product positioning, principles, users, v0.1 scope, and non-goals.
- `docs/ARCHITECTURE.md` — Module interfaces, backend seam, capabilities, streaming, extraction, and future locators.
- `docs/CLI.md` — Public command, JSON schema, stdout/stderr, and machine error contract.
- `docs/SECURITY.md` — Trust model, path rules, resource limits, staging, verification, and known limits.
