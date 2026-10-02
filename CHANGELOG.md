# Changelog

This file records user-visible changes to Arcthis. The project follows semantic versioning after the first public release.

## 0.5.1 - 2026-10-02

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
