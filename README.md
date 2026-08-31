# arcthis

**An agent-native command-line tool for accessing and changing compressed files.**

`arcthis` is one unified tool that lets humans and AI agents inspect, list, find, search, hash, read, extract, pack, and verify the contents of compressed files without first unpacking the whole thing.

[简体中文](./README.zh-CN.md)

## Why arcthis?

An agent should not have to fully extract a multi-gigabyte dataset just to find and read one `README.md`. `arcthis` treats an archive like a browsable file tree:

```sh
arcthis inspect dataset.tar.gz --json
arcthis tree dataset.tar.gz --json
arcthis stat dataset.tar.gz train/data.csv --json
arcthis find dataset.tar.gz --glob '**/*.csv' --json
arcthis read dataset.tar.gz train/data.csv | head
```

`read` prints one file's contents to stdout, so archive contents work with tools you already have:

```sh
arcthis read source.zip src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

"Not unpacking the whole archive" does not mean every format can jump to any file instantly. ZIP can usually read a single file directly; TAR and TAR.GZ must scan from start to finish. `inspect --json` reports what each format actually supports, so agents can plan around that cost.

## Project status

Arcthis v0.5.0 is publicly available from crates.io, GitHub Releases, Homebrew, npm, and pnpm. Every installation channel includes the CLI and the local MCP entry point by default.

Current commands:

- `list`, `tree`, `stat`, `inspect`, and `read` (which reads a file directly)
- glob-based `find`, capped line-by-line text search `grep`, and SHA-256/SHA-512 `hash`
- explicit nested-archive access with repeatable `--within`
- password-file access for encrypted ZIP and 7z archives
- explicit ordered split files with repeatable `--volume`
- a saved file-list cache (`index`) you can create, refresh, or delete
- safe full or single-file `extract`
- `extract-all` with a parallel-job cap and recursive discovery
- all-or-nothing `pack`, `--dry-run`, destination-collision policies, and `--delete-source`
- `verify` that checks as it reads
- a format `convert` that writes a temporary file and only saves it after re-verification
- a built-in local stdio MCP entry point: nine capped read-only tools and six plan-then-execute write tools that need explicit permission
- JSON output with a version number on every structured command

Planned commands and formats are kept in [ROADMAP.md](./ROADMAP.md) and are not presented as currently available.

## Supported formats

| Format | Detect | Access / extract | Create | How it reads |
| --- | --- | --- | --- | --- |
| ZIP | File signature | Stored/Deflate, including ZipCrypto/AES decryption | Deflate, unencrypted | Jump straight to a file |
| 7z | File signature | Yes, including AES decryption | LZMA2, unencrypted | Depends on block/solid |
| RAR / RAR5 | File signature | Read/extract through libarchive | No | Read in order; closed-format limits |
| TAR | Validated TAR header | Yes | Yes | Read in order |
| TAR.GZ / TGZ | Gzip signature plus TAR validation | Yes | Yes | Decompress in order |
| TAR.BZ2 / TBZ2 | Bzip2 signature plus TAR validation | Yes | Yes | Decompress in order |
| TAR.XZ / TXZ | XZ signature plus TAR validation | Yes | Yes | Decompress in order |
| TAR.ZST / TZST | Zstandard signature plus TAR validation | Yes | Yes | Decompress in order |
| GZIP | File signature, non-TAR content | One implicit file | Yes | Decompress in order |
| BZIP2 | File signature, non-TAR content | One implicit file | Yes | Decompress in order |
| XZ | File signature, non-TAR content | One implicit file | Yes | Decompress in order |
| Zstandard | File signature, non-TAR content | One implicit file | Yes | Decompress in order |

Detection is content-first for input archives; misleading input extensions do not override valid signatures. `pack` uses the output suffix you ask for to choose the new archive format.

The current ZIP build enables Stored/Deflate and AES decryption. Metadata listing can still identify a ZIP using another compression method, but reading or verifying that content returns `unsupported_operation` when the codec is unavailable. RAR is intentionally read-only; see [docs/RAR.md](./docs/RAR.md) for the underlying implementation, licensing, encryption, and native multipart limits.

## Install

Choose one public installation channel:

```sh
cargo install arcthis --locked
brew install mkynyd/tap/arcthis
npm install -g arcthis
pnpm add -g arcthis
```

pnpm 11 may hold packages published less than 24 hours ago. On release day, append `--config.minimumReleaseAge=0` if it reports `ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION`.

Prebuilt archives and SHA-256 checksums for Apple Silicon macOS, Intel macOS, and x86_64 Linux are available from the [v0.5.0 GitHub Release](https://github.com/mkynyd/arcthis/releases/tag/v0.5.0).

### Build from source

Rust 1.98.0 is pinned by `rust-toolchain.toml`.

RAR support links libarchive statically into release builds. Source builds need libarchive and its development dependencies. On macOS, install `libarchive libb2 bzip2 lz4 xz zstd` with Homebrew. On Debian/Ubuntu, install `libarchive-dev libb2-dev libbz2-dev liblz4-dev liblzma-dev libxml2-dev libzstd-dev zlib1g-dev`.

```sh
cargo build --release --locked
./target/release/arcthis --help
```

To install a local checkout into Cargo's binary directory:

```sh
cargo install --path . --locked
```

The default build includes the CLI, library, and local MCP entry point, so every installation channel provides the same commands:

```sh
arcthis mcp --allow-root ./archives
# Write tools become visible only with an explicit output policy:
arcthis mcp --allow-root ./archives --allow-output-root ./outputs
```

Library users who deliberately do not need MCP can disable default features with `--no-default-features`.

The stdio server pins MCP revision `2025-06-18`. `archive_read` requires an offset and length and returns at most the configured window. Source deletion additionally requires both `--allow-source-deletion` and `delete_source: true` in a plan/execute request. See [RFC 0003](./docs/RFC-0003-MCP-INTEGRATION.md) for the full rules.

## Quick start

```sh
# Discover before extracting
arcthis inspect archive.zip
arcthis list archive.zip
arcthis tree archive.zip --json

# Read one file directly
arcthis read archive.zip README.md

# Discover and search content before extraction
arcthis find source.tar.zst --glob '**/*.rs' --json
arcthis grep source.tar.zst TODO --glob '**/*.rs' --json
arcthis hash archive.zip model.bin --algorithm sha256

# Browse an inner archive without creating a temporary file
arcthis tree backup.zip --within project.tar.gz --json

# Read an encrypted archive without exposing the password in process arguments
arcthis read secret.7z data.txt --password-file ./password.txt

# Access an explicitly ordered split archive
arcthis inspect dataset.7z.001 --volume dataset.7z.002 --volume dataset.7z.003 --json

# Create or refresh the saved file-list cache
arcthis index dataset.7z --json

# Write a temporary sibling file, then safely save one file
arcthis extract archive.zip README.md --output ./README.md

# Safely extract all files
arcthis extract archive.tar.gz

# Review a destructive batch plan before anything runs
arcthis extract-all ./downloads --recursive --delete-source --dry-run --json

# Create, reopen, verify, and save a new archive
arcthis pack ./project --output project.tar.gz

# Verify every readable file
arcthis verify project.tar.gz --json

# Review the plan, write to a temporary location, verify, then save
arcthis convert project.zip --output project.tar.zst --dry-run --json
```

See [START.md](./START.md) for destination rules, resource limits, JSON formats, exit codes, and complete command guidance.

## Safety model

Extraction first checks metadata and paths, rejects links and special files, enforces declared and actual byte/time/ratio limits, writes into a temporary folder on the same filesystem, and saves the result only after every file succeeds. Existing destinations are refused by default; `--overwrite`, `--skip-existing`, and `--rename` are mutually exclusive explicit choices.

Packing writes a temporary sibling archive, finishes it, reopens it through the normal archive interface, verifies every file, and only then saves the requested output. An output inside a directory source, or any source/destination that points to the same place, is rejected. `--delete-source` runs only after that save, and only when deleting the source cannot remove the destination; dry-runs never write or delete.

Nested access decodes the selected inner file into a size-limited read-only memory buffer; it does not create a temporary file. Conversion writes validated files to a system temporary folder, packs, reopens, and verifies before saving. The saved file-list cache is treated as untrusted input and is invalidated by source size and modification time. Read [docs/SECURITY.md](./docs/SECURITY.md) for exact guarantees and known limits.

## Agent interface

- Successful structured output uses `schema_version: "1"`.
- Results go to stdout; warnings and errors go to stderr.
- Program errors are JSON on stderr when `--json` is active.
- `read` always emits raw bytes and rejects `--json`.
- BrokenPipe is treated as a successful early stop by the reader.
- File metadata explicitly reports `path_encoding` for non-UTF-8 names.
- File metadata includes stable archive order and lightweight extension-based file-type guesses.
- Non-TTY and JSON output contain no ANSI color decoration.

The public JSON format and error model are documented in [docs/CLI.md](./docs/CLI.md).

## Platform support

v0.5 is developed and tested on macOS and Linux through local tests and a two-platform GitHub Actions workflow, including all-feature MCP coverage. The design avoids Unix-only public interfaces, but Windows is not yet a supported automated-test target.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release --locked --all-features
```

Important entry points:

- [START.md](./START.md) — detailed user guide
- [INDEX.md](./INDEX.md) — concise repository map
- [docs/PRODUCT.md](./docs/PRODUCT.md) — product definition and non-goals
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — archive interface and underlying-format design
- [docs/SECURITY.md](./docs/SECURITY.md) — extraction and lifecycle security
- [AGENTS.md](./AGENTS.md) — long-lived repository rules for coding agents
- [CONTRIBUTING.md](./CONTRIBUTING.md) — contribution workflow

## Roadmap and contributing

The staged format and feature plan is in [ROADMAP.md](./ROADMAP.md); the six-stage MCP/remote/service/binding program is detailed in [docs/V0.5-INTEGRATIONS-PLAN.md](./docs/V0.5-INTEGRATIONS-PLAN.md). Contributions should preserve unified command behavior, direct read/write, JSON format compatibility, and conservative extraction defaults. Read [CONTRIBUTING.md](./CONTRIBUTING.md) before changing public behavior.

## License

`arcthis` is available under the [MIT License](./LICENSE).
Native and Rust dependency notices relevant to distribution are summarized in [THIRD_PARTY_LICENSES.md](./THIRD_PARTY_LICENSES.md).
