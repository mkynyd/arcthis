# Changelog

This file records user-visible changes to Arcthis. The project follows semantic versioning after the first public release.

## 0.6.0-rc.1 — 2026-10-16

### Compatibility

- Public JSON moves to schema `2`; published 0.5.1 remains schema `1`. CLI and MCP share read-only structures and typed error envelopes. Original name bytes, nullable executable state and timezone-less ZIP timestamps have explicit representations. See [ADR 0002](./docs/ADR-0002-EXPLORATION-CONTRACT.md).
- Filesystem result/plan references are canonical absolute paths; virtual stdin/nested names stay explicit. Existing named access rejects duplicate instances; `--entry-index` selects exactly one.
- Rust callers constructing complete `ArchiveOpenOptions`, `ExtractionLimits`, `ExtractAllOptions` or `ArchiveEntry` literals must account for the new fields/types.

### Fixed

- Reuse scan readers across ZIP, TAR-family, 7z and RAR; preserve selection/order/budgets, CRC errors and global stopping.
- Check solid-7z expansion by block; shared per-entry compressed sizes are unknown, not zero.
- Normalize safe dot paths before collision/path checks and align dry-run counts with extraction.
- Preserve ZIP duplicate directory instances, including AES/Deflate/ZIP64; refuse ambiguous named access and full extraction.
- Preserve instance identity through filename decoding and shared extraction, including raw byte names that share a `%XX` display with literal names.
- Fix cache deletion counts and Unix same-size/restored-mtime invalidation, metadata null/time semantics, argument/CRC diagnostics, human status/plurals/capabilities/deletion receipts and clean MCP EOF.

### Added

- `--entry-index`, strict `--encoding`, `read --verify-first`, `stat --verbose`, read-only stdin with byte limits, extraction `--no-sync`, `extract-all --continue-on-error`, `pack --strip-root`, and `index --list/--prune` with age/dry-run policies.
- Gzip FNAME/mtime read/write, split-name fallback rules, umask-respecting pack output and canonical lifecycle result paths.
- Root-authorized bounded per-request MCP password-file references on read-only tools; output schemas accept both versioned success and error results.
- ASCII root help mark (previous local work, retained).

## 0.5.1 - 2026-10-10

### Fixed

- Reject overlapping source and destination paths before writes, including conflicts between archives in recursive batches.
- Build deep file trees without recursion and reject paths above the supported depth or length.
- Apply MCP server file-count and decoded-byte limits to extract, pack, and convert requests.
- Report the enabled read-only or plan/execute mode correctly in MCP initialization.
- Preserve zero-prefixed content in Gzip, Bzip2, XZ, and Zstandard files. Empty compressed TAR archives use their explicit TAR suffix to resolve ambiguous zero blocks.

### Added

- MCP concurrent-call limit: four active calls by default, configurable with `--max-concurrent-requests`.
- MCP tool-response limit: 16 MiB by default, configurable with `--max-response-bytes`.
- `grep --max-result-bytes`: a 16 MiB default limit across matching lines. Omitted matches use the existing `matches_truncated` field.

### Compatibility

- Command names, result fields, and JSON schema version remain unchanged.
- Calls above a configured MCP limit return `resource_limit`; clients can retry when a concurrent call finishes.
- Rust callers using complete `ServiceLimits`, `GrepOptions`, or `McpConfig` struct literals must supply the new budget fields or use `..Default::default()`.

## 0.5.0 - 2026-08-31

First public release.

### Added

- Unified inspect, list, tree, stat, read, find, grep, hash, extract, pack, verify, convert, batch, nested-archive, multipart, password-file, and persistent-index workflows.
- ZIP, 7z, RAR/RAR5, TAR and compressed TAR families, plus single-stream Gzip, Bzip2, XZ, and Zstandard access.
- Stable versioned JSON output, machine-readable errors, finite resource limits, and Unix-friendly raw-byte reads.
- Built-in local MCP server with nine read-only tools and six explicitly authorized plan/execute write tools.
- GitHub Release preparation for Apple Silicon macOS, Intel macOS, and x86_64 Linux, including checksums, provenance attestations, shell, Homebrew, npm, and pnpm installation paths.

### Security

- Unified archive-path validation rejects traversal, absolute paths, Windows prefixes, invalid names, links, special files, duplicate paths, and file-parent conflicts.
- Extract, pack, convert, and destructive batch operations write to temporary locations and save only after validation and verification.
- Source deletion requires explicit permission and happens only after a verified save; source/destination aliases and destructive overlaps are rejected.
- MCP filesystem access is restricted to explicitly authorized roots, and write execution rejects stale plans or changed sources and destinations.

### Compatibility

- MCP is enabled by default so Cargo, GitHub, Homebrew, npm, and pnpm installations expose the same command set.
- Library-only users can disable MCP dependencies with `--no-default-features`.
- Windows and Linux arm64 remain unsupported until they have dedicated build and test coverage.
