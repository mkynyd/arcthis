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
- `THIRD_PARTY_LICENSES.md` — Rust and native dependency license summary for redistribution review.
- `.github/workflows/ci.yml` — macOS/Linux formatting, lint, test, and release-build workflow.
- `.gitignore` — Excludes build output, CodeGraph data, fixtures, temporary archives, `log.md`, and `HANDOFF.md`.

## Library and CLI

- `src/lib.rs` — Public library exports and machine schema version.
- `src/main.rs` — Thin binary entry point and process exit handoff.
- `src/cli.rs` — Clap grammar, command dispatch, BrokenPipe handling, JSON errors, and exit-code mapping.
- `src/model.rs` — Shared serialized archive, entry, capability, inspection, copy, and verification models.
- `src/error.rs` — Typed library errors and stable public error categories.
- `src/output.rs` — Human renderers and typed schema-versioned JSON envelopes.
- `src/query.rs` — Glob find, bounded streaming literal grep, and streaming digest operations.
- `src/index.rs` — Persistent entry metadata index with fingerprint invalidation and cache lifecycle.
- `src/convert.rs` — Staged conversion through safe extraction and verified packing with shared collision semantics.

## Archive access

- `src/archive/mod.rs` — Deep format-independent `Archive` interface, metadata index, and bounded nested traversal.
- `src/archive/locator.rs` — Filesystem archive source abstraction prepared for future locator types.
- `src/archive/detect.rs` — Content-first detection and compressed-prefix probing for every supported format.
- `src/archive/codec.rs` — Shared synchronous Gzip, Bzip2, XZ, and Zstandard decoder construction.
- `src/archive/backend/mod.rs` — Internal backend seam, repeatable file/memory sources, and entry-path rendering rules.
- `src/archive/backend/zip.rs` — ZIP metadata, streaming read/extract, and CRC verification adapter.
- `src/archive/backend/seven_zip.rs` — Pure-Rust 7z metadata, streaming access, extraction, and verification adapter.
- `src/archive/backend/tar.rs` — TAR and compressed-TAR sequential metadata, streaming access, extraction, and verification adapter.
- `src/archive/backend/stream.rs` — Single-codec-stream virtual-entry access and verification adapter.
- `src/archive/backend/rar.rs` — Read/extract/verify RAR and RAR5 adapter over statically linked libarchive.

## Materialization and lifecycle

- `src/security.rs` — Extraction path sanitizer and default resource-limit policy.
- `src/lifecycle.rs` — Shared collision resolution, staged commit, rollback, and post-commit source deletion.
- `src/extract.rs` — Extraction planning, intelligent destinations, staging writers, enforced limits, and commit.
- `src/batch.rs` — Content-based archive discovery and bounded deterministic `extract-all` execution.
- `src/pack.rs` — Multi-format source scan, staged creation, reopen verification, commit, and lifecycle planning.

## Tests

- `tests/cli_access.rs` — Detection, list/tree/stat/inspect/read, JSON errors, and BrokenPipe integration tests.
- `tests/cli_extract.rs` — Destination rules, single/full extraction, collision, traversal, link, duplicate, warning, and resource-limit regressions.
- `tests/cli_pack_verify.rs` — Container and single-stream pack/verify/extract round trips plus corruption regressions.
- `tests/cli_lifecycle.rs` — Dry-run, collision policy, delete-source, recursive batch, and worker regressions.
- `tests/cli_query.rs` — Find/grep/hash schemas, scan limits, binary detection, and in-memory nested traversal tests.
- `tests/cli_v04.rs` — Encrypted ZIP/7z, multipart byte-stream volumes, persistent index lifecycle, and convert regressions.

## Design documents

- `docs/PRODUCT.md` — Product positioning, principles, users, delivered milestones, and non-goals.
- `docs/ARCHITECTURE.md` — Module interfaces, backend seam, capabilities, streaming, extraction, and future locators.
- `docs/CLI.md` — Public command, JSON schema, stdout/stderr, and machine error contract.
- `docs/SECURITY.md` — Trust model, path rules, resource limits, staging, verification, and known limits.
- `docs/RFC-0001-NESTED-ARCHIVES.md` — Accepted explicit `--within` syntax, source model, and nested resource limits.
- `docs/RFC-0002-MULTIPART-SOURCES.md` — Accepted `--volume` byte-stream segment model and its format-native volume boundaries.
- `docs/RAR.md` — RAR backend, capabilities, encryption, licensing/redistribution, and native multipart limits.
