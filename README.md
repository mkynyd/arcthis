# arcthis

**An agent-native CLI for accessing and manipulating compressed files.**

`arcthis` is a unified archive access layer that lets humans and AI agents inspect, enumerate, stream, extract, pack, and verify compressed-file contents without materializing the entire archive by default.

[简体中文](./README.zh-CN.md)

## Why arcthis?

An agent should not need to fully extract a multi-gigabyte dataset just to discover and read one `README.md`. `arcthis` treats an archive as an accessible file tree:

```sh
arcthis inspect dataset.tar.gz --json
arcthis tree dataset.tar.gz --json
arcthis stat dataset.tar.gz train/data.csv --json
arcthis read dataset.tar.gz train/data.csv | head
```

`read` streams one entry to stdout, so archive contents compose with existing tools:

```sh
arcthis read source.zip src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

"Without materializing the entire archive" does not promise constant-time random access. ZIP can usually decode a selected entry directly; TAR and TAR.GZ require sequential scans. `inspect --json` reports the implemented capability model so agents can reason about that cost.

## Project status

The repository contains the implemented and tested v0.1 foundation. It is not yet published as a crates.io package or a prebuilt binary release.

Current commands:

- `list`, `tree`, `stat`, `inspect`, and streaming `read`
- safe complete or single-entry `extract`
- transactional `pack`
- full-stream `verify`
- schema-versioned JSON for every structured command

Planned commands and formats are kept in [ROADMAP.md](./ROADMAP.md) and are not presented as currently available.

## Supported formats

| Format | Detect | Access / extract | Create | Access model |
| --- | --- | --- | --- | --- |
| ZIP | Magic bytes | Stored and Deflate entries | Deflate | Random entry access |
| TAR | Validated TAR header | Yes | Yes | Sequential |
| TAR.GZ / TGZ | Gzip magic plus TAR validation | Yes | Yes | Sequential decompression |

Detection is content-first for input archives; misleading input extensions do not override valid signatures. `pack` uses the requested output suffix to choose the new archive format.

The current ZIP build intentionally enables Stored/Deflate support only. Metadata listing can still identify a ZIP that contains another compression method, but content access or verification returns `unsupported_operation` when the codec is unavailable.

## Install from source

Rust 1.98.0 is pinned by `rust-toolchain.toml`.

```sh
cargo build --release --locked
./target/release/arcthis --help
```

To install the current checkout into Cargo's binary directory:

```sh
cargo install --path . --locked
```

## Quick start

```sh
# Discover before extracting
arcthis inspect archive.zip
arcthis list archive.zip
arcthis tree archive.zip --json

# Stream one entry
arcthis read archive.zip README.md

# Extract one regular file through a temporary sibling file and commit
arcthis extract archive.zip README.md --output ./README.md

# Safely extract all entries
arcthis extract archive.tar.gz

# Create, reopen, verify, and commit a new archive
arcthis pack ./project --output project.tar.gz

# Verify every readable entry
arcthis verify project.tar.gz --json
```

See [START.md](./START.md) for destination rules, resource limits, JSON schemas, exit codes, and complete command guidance.

## Safety model

Extraction performs a complete metadata preflight, validates paths, rejects links and special files, enforces declared and actual byte limits, writes into a sibling staging location, and commits only after all entries succeed. Existing destinations are refused; v0.1 has no overwrite mode.

Packing writes a temporary sibling archive, finalizes it, reopens it through the normal archive interface, verifies every entry, and only then commits the requested output. The source is never deleted.

Read [docs/SECURITY.md](./docs/SECURITY.md) for exact guarantees and known limits. `--dry-run`, `--delete-source`, overwrite policies, nested archives, and encrypted archives are planned rather than partially implemented.

## Agent interface

- Successful structured output uses `schema_version: "1"`.
- Results go to stdout; warnings and errors go to stderr.
- Machine errors are JSON on stderr when `--json` is active.
- `read` always emits raw bytes and rejects `--json`.
- BrokenPipe is treated as a successful consumer stop.
- Entry metadata explicitly reports `path_encoding` for non-UTF-8 names.
- Non-TTY and JSON output contain no ANSI decoration.

The public schema and error model are documented in [docs/CLI.md](./docs/CLI.md).

## Platform support

v0.1 is developed and tested on macOS and Linux through local tests and a two-platform GitHub Actions workflow. The architecture avoids Unix-only public interfaces, but Windows is not yet a supported CI target.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release --locked
```

Important entry points:

- [START.md](./START.md) — detailed user guide
- [INDEX.md](./INDEX.md) — concise repository map
- [docs/PRODUCT.md](./docs/PRODUCT.md) — product definition and non-goals
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — archive interface and backend design
- [docs/SECURITY.md](./docs/SECURITY.md) — extraction and lifecycle security
- [AGENTS.md](./AGENTS.md) — long-lived repository rules for coding agents
- [CONTRIBUTING.md](./CONTRIBUTING.md) — contribution workflow

## Roadmap and contributing

The staged format and feature plan is in [ROADMAP.md](./ROADMAP.md). Contributions should preserve unified command semantics, streaming I/O, schema compatibility, and conservative extraction defaults. Read [CONTRIBUTING.md](./CONTRIBUTING.md) before changing public behavior.

## License

`arcthis` is available under the [MIT License](./LICENSE).
