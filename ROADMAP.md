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

- [x] 7z, Bzip2, XZ, Zstandard, TAR.BZ2, TAR.XZ, and TAR.ZST backends, including single-stream payloads.
- [x] `extract-all` with bounded archive-level workers and optional recursive discovery.
- [x] Machine-readable `--dry-run` execution plans.
- [x] Transactional `--delete-source` shared by pack and extract.
- [x] Mutually exclusive `--overwrite`, `--skip-existing`, and `--rename` policies.
- [x] Compression-ratio warnings plus optional ratio and per-entry duration enforcement.

## v0.3 — Agent content discovery

- [x] `find`, streaming literal `grep`, and streaming SHA-256/SHA-512 `hash`.
- [x] Binary detection plus per-entry size, line-size, and match-count scan limits.
- [x] Reusable in-process entry metadata index plus archive index and lightweight MIME metadata.
- [x] Nested archive locator RFC and bounded in-memory `--within` traversal support.
- [x] Non-persistent metadata indexing; persistent seek-point indexes remain a later measured optimization.

## v0.4 — Compatibility expansion

- [x] RAR read/extract evaluation with explicit licensing and redistribution analysis.
- [x] Encrypted archive/password interface.
- [x] Multipart archives.
- [x] Persistent indexes and cache lifecycle.
- [x] `convert` with verification and shared `--delete-source` semantics.

RAR creation is not assumed. Capabilities distinguish read, extract, create, and verify support; format-native RAR multi-volume traversal and RAR creation remain non-goals documented in `docs/RAR.md`.

## v0.5+ — Integrations

- MCP server and other agent-runtime frontends.
- Optional mount/TUI and human preview integrations.
- Remote HTTP/S3/SSH archive locators.
- HTTP interface, FFI, and language bindings if real consumers justify them.

FUSE is not an early milestone: direct `tree`, `stat`, `read`, and `find` operations are more controllable for agents.
