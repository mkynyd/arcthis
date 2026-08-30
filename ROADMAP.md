# Roadmap

The roadmap is capability-driven. Dates are intentionally omitted; a milestone ships only after its public interfaces, safety behavior, tests, and documentation agree.

## v0.1 — Trusted access foundation

- [x] Library-first archive interface and filesystem source.
- [x] Content-based detection for ZIP, TAR, and TAR.GZ.
- [x] `list`, `tree`, `stat`, `inspect`, and direct `read`.
- [x] Safe complete and single-file `extract` with intelligent destination selection.
- [x] Configurable file, total-size, and single-file extraction limits.
- [x] All-or-nothing `pack` for ZIP, TAR, and TAR.GZ.
- [x] Full-read `verify` using underlying integrity checks.
- [x] Stable version-1 JSON and stable error categories.
- [x] macOS/Linux automated-test configuration, security regression tests, and CLI integration tests.

## v0.2 — Formats and safe batch lifecycle

- [x] 7z, Bzip2, XZ, Zstandard, TAR.BZ2, TAR.XZ, and TAR.ZST format support, including single-file content.
- [x] `extract-all` with capped archive-level workers and optional recursive discovery.
- [x] Machine-readable `--dry-run` execution plans.
- [x] All-or-nothing `--delete-source` shared by pack and extract.
- [x] Mutually exclusive `--overwrite`, `--skip-existing`, and `--rename` policies.
- [x] Compression-ratio warnings plus optional ratio and per-file duration enforcement.

## v0.3 — Agent content discovery

- [x] `find`, literal `grep`, and SHA-256/SHA-512 `hash`.
- [x] Binary detection plus per-file size, line-size, and match-count scan limits.
- [x] Reusable in-process file metadata index plus archive index and lightweight MIME metadata.
- [x] Nested archive source RFC and capped in-memory `--within` traversal support.
- [x] Non-persistent metadata indexing; persistent seek-point indexes remain a later measured optimization.

## v0.4 — Compatibility expansion

- [x] RAR read/extract evaluation with explicit licensing and redistribution analysis.
- [x] Encrypted archive/password interface.
- [x] Explicit split-file sources through ordered `--volume` segments.
- [x] Persistent indexes and cache management.
- [x] `convert` with verification and shared `--delete-source` behavior.

RAR creation is not assumed. Capabilities distinguish read, extract, create, and verify support; format-native RAR multi-volume traversal and RAR creation remain non-goals documented in `docs/RAR.md`.

## v0.5 — Integration foundation and local MCP

- [x] Interface-independent synchronous application service with structured requests/results, cancellation checkpoints, decoded-byte budgets, result limits, and capped read windows.
- [x] Feature-gated local stdio MCP server pinned to protocol revision `2025-06-18` with nine read-only archive tools.
- [x] Explicit input-root policy; capped UTF-8/base64 file windows; JSON formats; structured errors; clean stdout; subprocess cancellation.
- [x] Opt-in extract, pack, and convert plan/execute tools protected by output roots, source/destination fingerprints, SHA-256 plan digests, all-or-nothing lifecycle rules, and disabled-by-default source deletion.
- [x] Official MCP Inspector compatibility, independent JSON-RPC subprocess tests, macOS/Linux all-feature automated tests, and real multi-format archive smoke tests.

The rules and verification evidence are described in [RFC 0003](./docs/RFC-0003-MCP-INTEGRATION.md) and the detailed [integration plan](./docs/V0.5-INTEGRATIONS-PLAN.md).

## v0.6+ — Further integrations

1. **v0.6:** capped HTTP remote archive source with validated range/cache behavior.
2. **v0.7:** authenticated S3 and SSH sources over the remote-source rules.
3. **v0.8:** versioned HTTP service with auth, quotas, cancellation, and streaming.
4. **v0.9:** small C ABI and demand-driven language bindings.
5. **post-v0.9:** optional preview/TUI and read-only mount evaluation.

Stage 1/v0.5 is complete. Stages 2–6 remain planned and require their own RFCs and quality gates. FUSE remains late because direct `tree`, `stat`, `read`, and `find` operations are more controllable for agents.
