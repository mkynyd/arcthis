# Roadmap

The roadmap is capability-driven. Dates are intentionally omitted; a milestone ships only after its public interfaces, safety semantics, tests, and documentation agree.

## v0.1 — Trusted access foundation

- [x] Library-first archive interface and filesystem locator.
- [x] Content-based detection for ZIP, TAR, and TAR.GZ.
- [x] `list`, `tree`, `stat`, `inspect`, and streaming `read`.
- [x] Safe complete and single-entry `extract` with intelligent destination selection.
- [x] Configurable entry, total-size, and single-entry extraction limits.
- [x] Transactional `pack` for ZIP, TAR, and TAR.GZ.
- [x] Full-stream `verify` using backend integrity checks.
- [x] Stable schema version 1 JSON and stable error categories.
- [x] macOS/Linux CI configuration, security regression tests, and CLI integration tests.

## v0.2 — Formats and safe batch lifecycle

- 7z, XZ, Zstandard, TAR.XZ, and TAR.ZST backend evaluation/implementation.
- `extract-all` with bounded archive-level workers and optional recursive discovery.
- Machine-readable `--dry-run` execution plans.
- Transactional `--delete-source` shared by pack and extract.
- Mutually exclusive `--overwrite`, `--skip-existing`, and `--rename` policies.
- Richer archive-bomb risk heuristics and optional CPU/time policy.

## v0.3 — Agent content discovery

- `find`, streaming `grep`, and streaming `hash`.
- Binary detection and per-entry scan limits.
- Partial-extraction performance improvements and richer metadata.
- Nested archive locator RFC and initial traversal support.
- Optional non-persistent indexes/seek points where measurements justify them.

## v0.4 — Compatibility expansion

- RAR read/extract evaluation with explicit licensing and redistribution analysis.
- Encrypted archive/password interface.
- Multipart archives.
- Persistent indexes and cache lifecycle.
- `convert` with verification and shared `--delete-source` semantics.

RAR creation is not assumed. Capabilities must distinguish read, extract, create, and verify support.

## v0.5+ — Integrations

- MCP server and other agent-runtime frontends.
- Optional mount/TUI and human preview integrations.
- Remote HTTP/S3/SSH archive locators.
- HTTP interface, FFI, and language bindings if real consumers justify them.

FUSE is not an early milestone: direct `tree`, `stat`, `read`, and `find` operations are more controllable for agents.
