# Repository Index

This map lists important maintained files and their current responsibilities. Generated output, local indexes, and fixture internals are intentionally omitted.

## Root

- `Cargo.toml` — Package metadata, restrained dependencies, release profile, and strict lint policy.
- `Cargo.lock` — Reproducible dependency resolution for builds and automated tests.
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

- `src/lib.rs` — Public library exports and machine format version.
- `src/app.rs` — Interface-independent structured application service, request limits, cancellation, capped reads, and recursive tree results.
- `src/main.rs` — Thin binary entry point and process exit handoff.
- `src/cli.rs` — Clap syntax, command dispatch, BrokenPipe handling, JSON errors, and exit-code mapping.
- `src/mcp.rs` — Feature-gated stdio MCP server, tool formats, root authorization, cancellation bridge, and transport handling.
- `src/mcp_mutation.rs` — Extract/pack/convert plan digests, source/destination fingerprints, and controlled execution handlers.
- `src/model.rs` — Shared serialized archive, file, capability, inspection, copy, and verification models.
- `src/error.rs` — Typed library errors and stable public error categories.
- `src/output.rs` — Human renderers and structured versioned JSON output.
- `src/query.rs` — Glob find, capped literal grep, and checksum operations.
- `src/index.rs` — Persistent file metadata index with fingerprint invalidation and cache management.
- `src/convert.rs` — Temporary-then-save conversion through safe extraction and verified packing with shared collision behavior.

## Archive access

- `src/archive/mod.rs` — Deep format-independent `Archive` interface, metadata index, and capped nested traversal.
- `src/archive/locator.rs` — Filesystem archive source abstraction prepared for future source types.
- `src/archive/detect.rs` — Content-first detection and compressed-prefix probing for every supported format.
- `src/archive/codec.rs` — Shared synchronous Gzip, Bzip2, XZ, and Zstandard decoder construction.
- `src/archive/backend/mod.rs` — Internal format-handling interface, repeatable file/memory sources, and file-path rendering rules.
- `src/archive/backend/zip.rs` — ZIP metadata, direct read/extract, and CRC verification handler.
- `src/archive/backend/seven_zip.rs` — Pure-Rust 7z metadata, direct access, extraction, and verification handler.
- `src/archive/backend/tar.rs` — TAR and compressed-TAR sequential metadata, direct access, extraction, and verification handler.
- `src/archive/backend/stream.rs` — Single-compression-format virtual-file access and verification handler.
- `src/archive/backend/rar.rs` — Read/extract/verify RAR and RAR5 handler over the native libarchive integration.

## Extraction and lifecycle

- `src/security.rs` — Extraction path safety check and default resource-limit policy.
- `src/lifecycle.rs` — Shared path-overlap checks, collision resolution, temporary-then-save, rollback, and post-save source deletion.
- `src/extract.rs` — Extraction planning, intelligent destinations, temporary-file writers, enforced limits, and save.
- `src/batch.rs` — Content-based archive discovery and capped deterministic `extract-all` execution.
- `src/pack.rs` — Multi-format source scan, temporary-file creation, reopen verification, save, and lifecycle planning.

## Tests

- `tests/cli_access.rs` — Detection, list/tree/stat/inspect/read, JSON errors, and BrokenPipe integration tests.
- `tests/cli_extract.rs` — Destination rules, single/full extraction, collision, traversal, link, duplicate, warning, and resource-limit regressions.
- `tests/cli_pack_verify.rs` — Container and single-file pack/verify/extract round trips plus corruption regressions.
- `tests/cli_lifecycle.rs` — Dry-run, collision policy, path-alias rejection, verified delete-source, recursive batch, and worker regressions.
- `tests/cli_query.rs` — Find/grep/hash formats, scan limits, binary detection, and in-memory nested traversal tests.
- `tests/cli_v04.rs` — RAR access, encrypted ZIP/7z, split files, persistent indexes, and convert regressions.
- `tests/app_service.rs` — Direct interface-independent service coverage and CLI-compatible structured results.
- `tests/mcp_stdio.rs` — Independent JSON-RPC subprocess coverage for discovery, limits, cancellation, root policy, and controlled mutation.
- `tests/mcp-inspector.json` — Official MCP Inspector launch configuration used for protocol/format smoke tests.

## Design documents

- `docs/PRODUCT.md` — Product positioning, principles, users, delivered milestones, and non-goals.
- `docs/ARCHITECTURE.md` — Module interfaces, format-handling interface, capabilities, direct reading, extraction, and future sources.
- `docs/CLI.md` — Public command, JSON format, stdout/stderr, and machine error rules.
- `docs/SECURITY.md` — Trust model, path rules, resource limits, temporary files, verification, and known limits.
- `docs/ADR-0001-TRANSACTIONAL-LIFECYCLE.md` — Accepted source/destination separation and verified delete-source invariants.
- `docs/RFC-0001-NESTED-ARCHIVES.md` — Accepted explicit `--within` syntax, source model, and nested resource limits.
- `docs/RFC-0002-MULTIPART-SOURCES.md` — Accepted `--volume` byte-stream segment model and its format-native volume boundaries.
- `docs/RFC-0003-MCP-INTEGRATION.md` — Accepted local MCP protocol, URI, authorization, capped content, cancellation, and mutation rules.
- `docs/RAR.md` — RAR implementation, capabilities, encryption, licensing/redistribution, and native multipart limits.
- `docs/V0.5-INTEGRATIONS-PLAN.md` — Completed v0.5 Stage 1 plus planned remote-source, service, binding, and human-integration stages.

## Website

- `site/index.html` — Chinese (default) landing page with real v0.5.0 command output, quickstart, and scroll-stacked signature-command cards.
- `site/download.html` / `site/docs.html` — Chinese source-build install guide and CLI reference.
- `site/en/` — English versions of all three pages. Language auto-detects from `navigator.language` on first visit and persists explicit choices via `localStorage`.
- `site/assets/style.css` — Shared light/dark semantic-token design system (system-adaptive with manual toggle).
- `site/assets/main.js` — Theme and language persistence, copy buttons, GSAP scroll reveals, and the window-scroll stacked-card effect.
- `site/assets/logo.svg` / `site/assets/favicon.svg` — Placeholder archive-tree logo mark.
- Deployed as static files to https://arcthis.mkynstudio.top (`/www/wwwroot/arcthis.mkynstudio.top` on the `remoteDev` host) via rsync; no build step.
