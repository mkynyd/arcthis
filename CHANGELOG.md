# Changelog

This file records user-visible changes to Arcthis. The project follows semantic versioning after the first public release.

## 0.5.0 - 2026-08-30

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
