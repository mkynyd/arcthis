# arcthis

**An agent-native CLI for accessing and manipulating compressed files.**

`arcthis` is a unified archive access layer that lets humans and AI agents inspect, enumerate, find, search, hash, stream, extract, pack, and verify compressed-file contents without materializing the entire archive by default.

[简体中文](./README.zh-CN.md)

## Why arcthis?

An agent should not need to fully extract a multi-gigabyte dataset just to discover and read one `README.md`. `arcthis` treats an archive as an accessible file tree:

```sh
arcthis inspect dataset.tar.gz --json
arcthis tree dataset.tar.gz --json
arcthis stat dataset.tar.gz train/data.csv --json
arcthis find dataset.tar.gz --glob '**/*.csv' --json
arcthis read dataset.tar.gz train/data.csv | head
```

`read` streams one entry to stdout, so archive contents compose with existing tools:

```sh
arcthis read source.zip src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

"Without materializing the entire archive" does not promise constant-time random access. ZIP can usually decode a selected entry directly; TAR and TAR.GZ require sequential scans. `inspect --json` reports the implemented capability model so agents can reason about that cost.

## Project status

The repository contains the implemented and tested v0.4 foundation. It is not yet published as a crates.io package or a prebuilt binary release.

Current commands:

- `list`, `tree`, `stat`, `inspect`, and streaming `read`
- glob-based `find`, bounded streaming literal `grep`, and SHA-256/SHA-512 `hash`
- explicit, bounded nested archive traversal with repeatable `--within`
- password-file access for encrypted ZIP and 7z archives
- explicit ordered byte-stream volumes with repeatable `--volume`
- persistent entry metadata indexes with an explicit cache lifecycle
- safe complete or single-entry `extract`
- bounded `extract-all` with recursive discovery
- transactional `pack`, `--dry-run`, collision policies, and `--delete-source`
- full-stream `verify`
- verified staged archive `convert`
- schema-versioned JSON for every structured command

Planned commands and formats are kept in [ROADMAP.md](./ROADMAP.md) and are not presented as currently available.

## Supported formats

| Format | Detect | Access / extract | Create | Access model |
| --- | --- | --- | --- | --- |
| ZIP | Magic bytes | Stored/Deflate, including ZipCrypto/AES decryption | Deflate, unencrypted | Random entry access |
| 7z | Magic bytes | Yes, including AES decryption | LZMA2, unencrypted | Block/solid dependent |
| RAR / RAR5 | Magic bytes | Read/extract through libarchive | No | Sequential; proprietary-format limitations |
| TAR | Validated TAR header | Yes | Yes | Sequential |
| TAR.GZ / TGZ | Gzip magic plus TAR validation | Yes | Yes | Sequential decompression |
| TAR.BZ2 / TBZ2 | Bzip2 magic plus TAR validation | Yes | Yes | Sequential decompression |
| TAR.XZ / TXZ | XZ magic plus TAR validation | Yes | Yes | Sequential decompression |
| TAR.ZST / TZST | Zstandard magic plus TAR validation | Yes | Yes | Sequential decompression |
| GZIP | Magic bytes, non-TAR payload | One implicit entry | Yes | Sequential |
| BZIP2 | Magic bytes, non-TAR payload | One implicit entry | Yes | Sequential |
| XZ | Magic bytes, non-TAR payload | One implicit entry | Yes | Sequential |
| Zstandard | Magic bytes, non-TAR payload | One implicit entry | Yes | Sequential |

Detection is content-first for input archives; misleading input extensions do not override valid signatures. `pack` uses the requested output suffix to choose the new archive format.

The current ZIP build enables Stored/Deflate and AES decryption. Metadata listing can still identify a ZIP using another compression method, but content access or verification returns `unsupported_operation` when the codec is unavailable. RAR is intentionally read-only; see [docs/RAR.md](./docs/RAR.md) for backend, licensing, encryption, and native multipart limits.

## Install from source

Rust 1.98.0 is pinned by `rust-toolchain.toml`.

RAR support links libarchive statically into release builds. Source builds need libarchive and its development dependencies. On macOS, install `libarchive libb2 bzip2 lz4 xz zstd` with Homebrew. On Debian/Ubuntu, install `libarchive-dev libb2-dev libbz2-dev liblz4-dev liblzma-dev libxml2-dev libzstd-dev zlib1g-dev`.

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

# Discover and search content before extraction
arcthis find source.tar.zst --glob '**/*.rs' --json
arcthis grep source.tar.zst TODO --glob '**/*.rs' --json
arcthis hash archive.zip model.bin --algorithm sha256

# Traverse an inner archive without creating a temporary inner file
arcthis tree backup.zip --within project.tar.gz --json

# Read an encrypted archive without exposing the secret in process arguments
arcthis read secret.7z data.txt --password-file ./password.txt

# Access an explicitly ordered byte-split archive
arcthis inspect dataset.7z.001 --volume dataset.7z.002 --volume dataset.7z.003 --json

# Create or refresh a persistent metadata index
arcthis index dataset.7z --json

# Extract one regular file through a temporary sibling file and commit
arcthis extract archive.zip README.md --output ./README.md

# Safely extract all entries
arcthis extract archive.tar.gz

# Inspect a destructive batch plan before execution
arcthis extract-all ./downloads --recursive --delete-source --dry-run --json

# Create, reopen, verify, and commit a new archive
arcthis pack ./project --output project.tar.gz

# Verify every readable entry
arcthis verify project.tar.gz --json

# Convert through safety-checked staging, then verify before commit
arcthis convert project.zip --output project.tar.zst --dry-run --json
```

See [START.md](./START.md) for destination rules, resource limits, JSON schemas, exit codes, and complete command guidance.

## Safety model

Extraction performs a complete metadata preflight, validates paths, rejects links and special files, enforces declared and actual byte/time/ratio limits, writes into a sibling staging location, and commits only after all entries succeed. Existing destinations are refused by default; `--overwrite`, `--skip-existing`, and `--rename` are mutually exclusive explicit policies.

Packing writes a temporary sibling archive, finalizes it, reopens it through the normal archive interface, verifies every entry, and only then commits the requested output. Pack destinations inside a directory source and any source/destination alias are rejected. `--delete-source` runs only after that commit and only when deleting the source cannot remove the destination; dry-runs never write or delete.

Nested access decodes the selected inner entry into a resource-bounded immutable memory source; it does not create a named temporary inner archive. Conversion deliberately materializes validated entries inside a system temporary staging directory before verified packing. Persistent metadata indexes are treated as untrusted cache input and invalidated by source size and modification time. Read [docs/SECURITY.md](./docs/SECURITY.md) for exact guarantees and known limits.

## Agent interface

- Successful structured output uses `schema_version: "1"`.
- Results go to stdout; warnings and errors go to stderr.
- Machine errors are JSON on stderr when `--json` is active.
- `read` always emits raw bytes and rejects `--json`.
- BrokenPipe is treated as a successful consumer stop.
- Entry metadata explicitly reports `path_encoding` for non-UTF-8 names.
- Entry metadata includes stable archive order and lightweight extension-based MIME guesses.
- Non-TTY and JSON output contain no ANSI decoration.

The public schema and error model are documented in [docs/CLI.md](./docs/CLI.md).

## Platform support

v0.4 is developed and tested on macOS and Linux through local tests and a two-platform GitHub Actions workflow. The architecture avoids Unix-only public interfaces, but Windows is not yet a supported CI target.

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

The staged format and feature plan is in [ROADMAP.md](./ROADMAP.md); the six-stage MCP/remote/service/binding program is detailed in [docs/V0.5-INTEGRATIONS-PLAN.md](./docs/V0.5-INTEGRATIONS-PLAN.md). Contributions should preserve unified command semantics, streaming I/O, schema compatibility, and conservative extraction defaults. Read [CONTRIBUTING.md](./CONTRIBUTING.md) before changing public behavior.

## License

`arcthis` is available under the [MIT License](./LICENSE).
Native and Rust dependency notices relevant to distribution are summarized in [THIRD_PARTY_LICENSES.md](./THIRD_PARTY_LICENSES.md).
