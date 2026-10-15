# Using arcthis

[简体中文](./START.zh-CN.md)

This guide covers **0.6.0 (JSON schema 2)**. Version 0.5.1 used schema 1; migrate consumers using [ADR 0002](./docs/ADR-0002-EXPLORATION-CONTRACT.md). Changes and validation are recorded in the [iteration acceptance](./docs/EXPLORATION-ITERATION-PLAN.md).

v0.5.1 added write-path overlap checks, file-tree depth limits, MCP concurrent-call and response limits, and `grep --max-result-bytes`. That release retained its prior command names and JSON fields; the current source migration is described above.

Detection uses file content. Empty compressed TAR files have only zero blocks, so keep their `.tar.gz`, `.tar.bz2`, `.tar.xz`, `.tar.zst` or shorthand suffix. Other zero-prefixed compressed files remain single-file streams.

## Source-checkout additions in 0.6.0

```sh
arcthis read bundle.zip --entry-index 3 --verify-first
arcthis stat legacy.zip --encoding gbk --entry-index 0 --verbose
cat bundle.zip | arcthis list - --json
arcthis extract files.tar --output out --no-sync
arcthis extract-all incoming --continue-on-error --json
arcthis pack folder --strip-root --output contents.zip
arcthis index --list --json
arcthis index --prune --dry-run --max-age-seconds 2592000 --json
```

Index selection applies only at the final archive in a within chain. Stat/read/hash may omit entry if one regular file exists or an index is given. Name decoding is strict (utf8/cp437/gbk/shift-jis); original bytes stay in path_raw_hex and decoded names still undergo extraction safety checks. Absolute timestamps are RFC3339 or null; timezone-less ZIP times use modified_time_local. Directory executable/CRC and unknown metadata are null.

Verified reads use anonymous temporary storage and enforce a 4 GiB default actual-byte limit; default raw streaming may output partial bytes before failure, so check exit status. Stdin compressed data is buffered for seek/reopen with a 64 MiB default max-stdin-bytes limit and optional stdin-name; it is read-only and cannot be mixed with volumes. No-sync trades crash durability for speed while retaining staging and lifecycle checks. Continue-on-error reports plan.errors and partial_failure/12 while preserving source/destination overlap checks. Pack output follows umask and Gzip FNAME/mtime survives renaming.

Index inventory/pruning affects only hash-named documents in the index cache; default expiry is 30 days, dry-run never deletes, and unrelated files remain. Unix ctime/dev/inode catches common same-size/restored-mtime replacements. archive.index_used records cache consumption; cache fingerprints are not content authenticity proofs. `read --json` is invalid_argument/2. Lifecycle source/destination paths are canonical absolute paths in both plan and result.

MCP read-only requests accept password_file inside an authorized input root (at most 4096 bytes), not an inline password. Clean EOF exits 0; --json does not apply to the JSON-RPC server. CLI and MCP success/error envelopes use schema 2. See the [complete contract](./docs/CLI.md) and [migration ADR](./docs/ADR-0002-EXPLORATION-CONTRACT.md).

## Install

Choose one public installation channel. Every option includes MCP by default:

```sh
cargo install arcthis --locked
brew install mkynyd/tap/arcthis
npm install -g arcthis
pnpm add -g arcthis
```

If pnpm 11 reports `ERR_PNPM_IGNORED_BUILDS`, approve the Arcthis installation script by appending `--allow-build=arcthis` to the install command.

pnpm 11 may hold packages published less than 24 hours ago. On release day, append `--config.minimumReleaseAge=0` if it reports `ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION`.

The npm/pnpm installer downloads the binary from GitHub. On networks requiring a proxy, pass the actual proxy address through `HTTPS_PROXY`/`HTTP_PROXY` for the installation command; Node does not automatically inherit macOS system proxy settings.

Prebuilt archives for Apple Silicon macOS, Intel macOS, and x86_64 Linux are attached to the [v0.6.0 GitHub Release](https://github.com/mkynyd/arcthis/releases/tag/v0.6.0).

### Build from source

The repository pins Rust 1.98.0.

RAR support uses statically linked libarchive. Install native build dependencies first: Homebrew `libarchive libb2 bzip2 lz4 xz zstd` on macOS, or `libarchive-dev libb2-dev libbz2-dev liblz4-dev liblzma-dev libxml2-dev libzstd-dev zlib1g-dev` on Debian/Ubuntu.

```sh
cargo build --release --locked
./target/release/arcthis --version
```

Install a local checkout with:

```sh
cargo install --path . --locked
```

## Local MCP entry point

MCP is enabled in the default build so Cargo, Homebrew, npm, and pnpm installations expose the same command set. Start it by authorizing one or more input roots:

```sh
./target/release/arcthis mcp --allow-root ./archives
```

Library users can still opt out with `cargo build --no-default-features`.

The stdio transport uses protocol revision `2025-06-18` and reserves stdout for JSON-RPC. It exposes `archive_inspect`, `archive_list`, `archive_tree`, `archive_stat`, `archive_read`, `archive_find`, `archive_grep`, `archive_hash`, and `archive_verify`. Every request has finite limits on files (`--max-entries`, 100,000), decoded bytes (`--max-decoded-bytes`, 1 GiB), and results (`--max-results`, 10,000); `archive_read` additionally requires `offset` and `length` and defaults to a 1 MiB maximum window (`--max-read-window`).

Extract, pack, and convert each use separate `_plan` and `_execute` tools. They are absent unless at least one `--allow-output-root` is configured. Execution requires the exact SHA-256 plan digest and rejects source or destination changes since planning. Source deletion is disabled unless both `--allow-source-deletion` is present at server launch and the request explicitly asks for deletion. Password values are not accepted in MCP requests.

```sh
./target/release/arcthis mcp \
  --allow-root ./archives \
  --allow-output-root ./outputs
```

See [RFC 0003](./docs/RFC-0003-MCP-INTEGRATION.md) for authorization, JSON formats, cancellation, and binary transport details.

MCP defaults to four active tools and a 16 MiB tool-response budget. Configure these with `--max-concurrent-requests` (1–64) and `--max-response-bytes` (at least 1024; at least 131072 with output roots). The budget includes structured and text JSON copies. Excess concurrent calls or oversized responses return `resource_limit`; retry busy calls after an active call completes, and narrow large queries or use read windows. Grep's retained-match budget is at most one quarter of the response ceiling, and scan bytes across files share `--max-decoded-bytes`.

## Core workflow

Use access operations before extraction:

```text
inspect -> list/tree -> stat -> read -> extract only when needed
```

For an agent, a typical sequence is:

```sh
arcthis inspect dataset.tar.gz --json
arcthis tree dataset.tar.gz --json
arcthis stat dataset.tar.gz train/data.csv --json
arcthis read dataset.tar.gz train/data.csv | head -n 20
```

Input formats are detected from content. The current build supports ZIP, 7z, RAR/RAR5, TAR, compressed TAR variants using Gzip/Bzip2/XZ/Zstandard, and the same four compression methods as single-file formats. Compressed TAR, single-file formats, RAR, and solid 7z may scan or decode preceding data; `inspect` reports the actual access cost.

## Root help in the current checkout

Run `arcthis`, `arcthis -h`, or `arcthis --help` to see the ASCII logo, introduction, and root command list. A bare invocation retains the missing-command behavior: help goes to stderr and the process exits with code 2; explicit help goes to stdout and exits with code 0. Subcommand help, version output, archive results, and MCP responses have no banner. The logo is plain ASCII without ANSI color. The logo was added in v0.6.0.

## Command shape and global options

```text
arcthis <command> <archive> [entry] [options]
```

Global options can appear with a subcommand:

- `--json` emits a versioned machine result where supported.
- `--no-color` disables terminal color decoration. Output does not require color.
- repeatable `--within <entry>` enters explicit nested archives for access/query commands.
- `--max-nested-entry-size <bytes>` caps each decoded inner archive (256 MiB by default).
- `--password-file <path>` reads a password without exposing it as a process argument; trailing CR/LF is removed.
- repeatable `--volume <path>` appends explicitly ordered split files after the primary archive path.
- `--index-directory <path>` overrides the platform cache root used by the saved file-list cache.
- `-h`, `--help` shows command help.
- `-V`, `--version` shows the version.

`NO_COLOR`, non-TTY output, and JSON output contain no ANSI color decoration. The current build emits no ANSI decoration at all, so `--no-color` and `NO_COLOR` are accepted for compatibility and change nothing today.

## `inspect` — learn archive cost and risk

```sh
arcthis inspect archive.tar.gz
arcthis inspect archive.tar.gz --json
```

`inspect` lists archive metadata without reading every file's content. Human output reports format, compression, entry count, declared sizes, access cost, volume state, and any warnings. `--json` additionally exposes the compression ratio, the `encrypted`/`solid` flags, the `capabilities` object, and the full warning list.

Important warning codes include:

- `sequential_access` — selected reads may scan from the beginning.
- `duplicate_entry_paths` — repeated display paths, including ZIP instances, require explicit index selection and prevent full extraction.
- `encrypted_entries` — encrypted content is present and content operations need the correct password.
- `non_regular_entries` — links or special entries will be rejected by extraction.
- `unsafe_entry_paths` — extraction path validation would reject at least one entry.
- `default_extraction_limits_exceeded` — declared metadata exceeds default limits.
- `high_compression_ratio` — at least one entry declares expansion above 1000:1.
- `single_stream_metadata_scan` — determining the implicit content size required sequential decoding.
- `multipart_byte_stream` — the source combines explicitly ordered split files.
- `rar_metadata_limited` — the current RAR implementation cannot always report solid, encryption, or compressed-size metadata.

Inspection warnings inform planning. The extraction path independently enforces the corresponding checks.

## `list` — list files

```sh
arcthis list archive.zip
arcthis list archive.zip --json
```

Human output is a tab-separated `KIND`, `SIZE`, and `PATH` table. JSON preserves all archive instances, including repeated ZIP central-directory names. Use `archive_index` / `--entry-index` to select one instance; path-only selection refuses ambiguity.

A file object includes:

- `archive_index`, preserving source archive order;
- `path` and `path_encoding` (utf8/escaped_bytes/cp437/gbk/shift_jis), plus `path_raw_hex`;
- `kind`: `file`, `directory`, `symlink`, `hardlink`, or `other`;
- `size` and optional `compressed_size`;
- optional `modified_time`;
- `encrypted`, `executable`, optional `symlink_target`, and optional `crc32`.
- optional `mime_guess`, inferred from the path extension without reading content.

Invalid UTF-8 entry bytes are represented with `%XX` escaping and `path_encoding: "escaped_bytes"`. They can be listed and addressed by the displayed value. Extraction refuses to write them because it cannot preserve the original filesystem name unambiguously.

## `tree` — view the logical file tree

```sh
arcthis tree source.tar
arcthis tree source.tar --json
```

Human output uses tree characters. JSON returns recursive nodes with `name`, logical `path`, `kind`, an optional source `entry`, and `children`. Implicit directories have `entry: null`; duplicate file leaves remain separate where the backend exposes them. Tree paths are limited to 4096 bytes and 256 non-empty components; larger paths return `resource_limit` instead of building an unbounded result.

## `stat` — inspect one named file

```sh
arcthis stat archive.zip README.md
arcthis stat archive.zip README.md --json
```

The path must match `list`. A missing path returns `entry_not_found`; repeated display paths return `collision`. `--entry-index N` selects an exact instance. For stat/read/hash, entry may be omitted when only one regular file exists or an index is explicit; the index applies to the final archive after `--within`.

## `read` — read one file directly

```sh
arcthis read archive.zip README.md
arcthis read source.tar.gz src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

`read` is the core content command. It writes only raw file bytes to stdout and writes diagnostics to stderr. It never wraps bytes in JSON and therefore rejects `--json` with `invalid_argument` (exit 2).

Regular files are supported. Directories, links, and special entries are rejected for `read`. BrokenPipe exits successfully, so `arcthis read ... | head` is a normal workflow.

For compressed TAR and solid 7z, the command may decode preceding data. Single-file formats expose one file derived from the filename, such as `report.txt.gz` → `report.txt`. None of these operations write the full archive to disk.

## `find` — filter file paths

```sh
arcthis find dataset.7z --glob '**/*.json'
arcthis find dataset.7z --glob '**/*.json' --json
```

`find` matches the full normalized archive path and returns complete file metadata without decoding file content.

## `grep` — capped content search

```sh
arcthis grep source.tar.gz TODO --glob '**/*.rs'
arcthis grep papers.zip transformer --glob '**/*.md' --json
```

The pattern is a raw byte sequence, not a regular expression. Files above `--max-entry-size` are skipped (16 MiB default), collection stops at `--max-matches` (10,000 default), and retained lines are capped at 1 MiB. A NUL in the first 8 KiB classifies a file as binary; binary files are skipped unless `--binary` is set. JSON reports scan, skip, byte, and truncation counters.

ZIP searches reuse one central directory; TAR-family, 7z and RAR searches reuse one content traversal. Glob/declared-size filtering runs before selected entry content is copied. Sequential formats may still decode preceding bytes to reach selected entries. The visitor preserves scan/result budgets and archive instance identity without materializing files.

`--max-result-bytes` caps serialized matching-line objects across files (16 MiB default), including path overhead, escaping, and UTF-8 replacements. If a complete match cannot fit, it is omitted and `matches_truncated` becomes `true`; zero collects no matches. Increase this limit explicitly when more output is needed.

## `hash` — checksum one file

```sh
arcthis hash models.zip model.bin
arcthis hash models.zip model.bin --algorithm sha512 --json
```

SHA-256 is the default and SHA-512 is available. The file is streamed into the checksum and never written to disk.

## Encrypted archives with `--password-file`

```sh
arcthis inspect private.zip --json
arcthis read private.zip report.txt --password-file ./password.txt
arcthis verify private.7z --password-file ./password.txt --json
```

Passwords are never accepted as ordinary command-line arguments. The password file is read as bytes and final CR/LF bytes are removed, making a one-line secret file convenient. ZIP supports ZipCrypto and AES decryption; 7z supports AES decryption. Archive creation remains unencrypted, so `pack --password-file` returns `unsupported_operation` instead of silently ignoring the option. Missing and incorrect passwords use the stable `password_required` and `wrong_password` categories.

RAR accepts the same interface, but actual encrypted-RAR support depends on libarchive and the RAR variant. Unsupported encryption returns an explicit implementation error. See [docs/RAR.md](./docs/RAR.md).

## Split files with `--volume`

```sh
arcthis inspect dataset.7z.001 \
  --volume dataset.7z.002 \
  --volume dataset.7z.003 \
  --json
arcthis read dataset.7z.001 data.csv \
  --volume dataset.7z.002 \
  --volume dataset.7z.003
```

The positional archive is the first volume. Every `--volume` is appended in the exact order supplied and the combined seekable byte stream is passed to normal format detection. Paths must be unique and all volumes must exist. This supports archives split at byte boundaries, such as a split 7z stream; it does not pretend that native RAR volume sets are simple concatenations. `inspect` reports `multipart` and `volume_count`. Source deletion is rejected for split extraction/conversion because deleting several source volumes cannot yet provide the single-source lifecycle guarantee. See [RFC 0002](./docs/RFC-0002-MULTIPART-SOURCES.md).

## `index` — manage a saved file-list cache

```sh
arcthis index dataset.7z --json
arcthis index dataset.7z --refresh --json
arcthis index dataset.7z --delete --dry-run --json
arcthis index dataset.7z --delete --json
```

`index` stores listed file metadata in the platform cache directory. Later opens reuse a valid cache automatically. The cache key uses the normalized source path; source size and nanosecond modification time invalidate stale entries. `--refresh` forces a re-list and a single-step replacement. `--delete` removes only this archive's cache, and `--dry-run` reports `would_create`, `would_refresh`, `would_reuse`, or `would_delete` — and `missing` when `--delete` finds no existing index — without changing cache files. Use `--index-directory` to isolate or relocate the cache.

Cache files are untrusted optimization data: malformed, format-mismatched, or stale documents are ignored. Caches contain metadata, not decoded content or TAR seek points.

## Nested archives with `--within`

```sh
arcthis tree backup.zip --within project.tar.gz --json
arcthis read backup.zip README.md --within project.tar.gz
arcthis grep bundle.zip TODO --within layer.7z --within source.tar.zst
```

Each `--within` names an archive file in the current level. The file is decoded into a capped read-only memory buffer and opened through normal format detection; no named temporary file is created. Maximum depth is 8 and each level defaults to 256 MiB. Nested extraction and conversion are unsupported in v0.5. See [RFC 0001](./docs/RFC-0001-NESTED-ARCHIVES.md).

## `extract` — safely extract content

### Extract all files

```sh
arcthis extract archive.zip
arcthis extract archive.tar.gz --output ./restored
```

An explicit `--output` is the complete extraction root. Without it:

- if every file is under one real top-level directory, that directory is saved directly;
- otherwise, a directory is derived from the complete archive suffix (`backup.tar.gz` becomes `backup/`);
- the destination must not already exist unless an explicit collision handling is selected.

Examples:

```text
archive: project/README.md, project/src/lib.rs
result:  ./project/README.md, ./project/src/lib.rs

archive: README.md, src/lib.rs
input:   bundle.tar.gz
result:  ./bundle/README.md, ./bundle/src/lib.rs
```

### Extract one regular file

Single-file extraction requires an explicit output file:

```sh
arcthis extract archive.zip README.md --output ./README.md
```

The command writes a temporary sibling file, flushes and syncs it, and then applies the selected save policy. Directory or link entries are unsupported.

### Resource limits

```sh
arcthis extract archive.zip \
  --max-entries 50000 \
  --max-total-size 8589934592 \
  --max-entry-size 2147483648 \
  --max-compression-ratio 1000 \
  --max-entry-duration-seconds 300
```

Values are raw bytes/counts. Defaults are:

| Option | Default |
| --- | ---: |
| `--max-entries` | 100,000 |
| `--max-total-size` | 16 GiB |
| `--max-entry-size` | 4 GiB |
| `--max-compression-ratio` | Disabled unless specified |
| `--max-entry-duration-seconds` | Disabled unless specified |

Declared metadata is checked before writing the temporary file. Actual bytes are counted while reading. A violation returns `resource_limit` and does not save the destination.

### Planning, collisions, and source lifecycle

Use `--dry-run` to emit the same resolved destination, collision state, warnings, estimated size, and delete intent without writing or deleting anything:

```sh
arcthis extract archive.tar.zst --dry-run --delete-source --json
```

The default collision handling refuses an existing destination. Select exactly one alternative when needed:

- `--overwrite` replaces the destination in one step and restores the previous path if the save fails;
- `--skip-existing` reports a successful skipped operation and never deletes the source;
- `--rename` chooses the first available numbered sibling such as `bundle.1`.

`--delete-source` runs only after extraction has fully written the temporary file, verified the complete source archive, and saved the result. This complete verification also applies when extracting one selected file, so deleting the source can require decoding unselected files. Any planning, decoding, verification, write, or save failure preserves the source archive. Source/destination aliases and every ancestor/descendant overlap are rejected as `collision` before writing, even when source deletion is not requested.

### Extraction safety

Extraction normalizes `.` and redundant internal separators, then rejects every `..`, absolute/drive/UNC paths, NUL/backslashes, normalized duplicates/case collisions/file parents, excessive paths, unresolved non-UTF-8 names, and links/special files. ZIP duplicates are visible and refused. Writes remain staged on the destination filesystem; default per-file fsync can be explicitly skipped with `--no-sync`, trading crash durability for speed without removing rollback or verified source-deletion rules.

## `extract-all` — process a directory of archives

```sh
arcthis extract-all ./downloads --dry-run --json
arcthis extract-all ./downloads --recursive --workers 4
arcthis extract-all ./downloads --recursive --delete-source
```

Discovery identifies supported archives by content rather than suffix. The default scans only the named directory; `--recursive` descends through filesystem directories, not archives nested inside archives. `--workers` caps concurrent independent archive operations from 1 to 64.

The command plans every archive before execution and rejects equal or nested destination conflicts across the batch, including a destination that contains another discovered archive source. Each archive receives the same resource limits and collision handling as `extract`. A mixed outcome returns `partial_failure`; JSON reports a deterministic, path-sorted item result for every discovered archive.

## `pack` — create and verify an archive

```sh
arcthis pack ./project --output project.zip
arcthis pack ./project --output project.7z
arcthis pack ./project --output project.tar.zst --json
arcthis pack ./report.txt --output report.txt.xz
```

The output suffix selects ZIP, 7z, TAR, TAR.GZ/TGZ, TAR.BZ2/TBZ2, TAR.XZ/TXZ, TAR.ZST/TZST, or a single Gzip/Bzip2/XZ/Zstandard compressed file. Single compressed files require a regular source file. Directory packing includes the source directory itself as the top-level entry, preserving empty directories and regular files. ZIP output uses Deflate.

`pack` currently creates unencrypted output. Passing `--password-file`, `--volume`, or `--within` returns `unsupported_operation` instead of silently ignoring the option.

Symlinks and special files are rejected. The default destination policy refuses a collision; `--overwrite`, `--skip-existing`, and `--rename` have the same meanings as extraction. `--dry-run` returns the resolved plan. The process is:

```text
scan source -> write temporary sibling -> finish -> sync -> reopen -> verify -> save -> optionally delete source
```

With `--delete-source`, the source is removed only after the saved archive reopens and verifies successfully. Every earlier failure preserves it.

The output must be outside a directory source, and it may not resolve to the same path as a file source. This prevents an archive from including, replacing, or later deleting its own destination.

## `convert` — change archive format with a verified process

```sh
arcthis convert backup.zip --output backup.tar.zst --dry-run --json
arcthis convert backup.zip --output backup.7z --delete-source
arcthis convert data.7z.001 --volume data.7z.002 --output data.tar.zst
```

The output suffix selects the same writable formats as `pack`; RAR creation is intentionally unsupported. v0.4 conversion uses `staged_materialization` (write to a temporary location first, then save): it opens the source through the unified implementation, enforces extraction path and resource limits, writes validated regular files into a system temporary directory, packs a temporary target, reopens and verifies it, saves it under the selected collision handling, and only then optionally deletes the single source archive.

```text
open -> validate -> write files to temporary folder -> pack/finish -> reopen/verify -> save -> optionally delete source
```

`--dry-run` performs source listing, path/resource validation, target-format validation, and collision handling, then emits a structured plan without creating a target or temporary folder. Conversion preserves archive file paths rather than adding the temporary folder name. A single compressed file target (`.gz`, `.bz2`, `.xz`, or `.zst`) requires exactly one root-level regular file and no other files.

Conversion accepts the same `--overwrite`, `--skip-existing`, `--rename`, extraction limit, and `--password-file` options. Compound suffix rename is preserved (`backup.tar.zst` becomes `backup.1.tar.zst`). Nested conversion and split `--delete-source` are rejected. Any failure before the target is saved preserves the source.

## `verify` — check readable archive data

```sh
arcthis verify archive.zip
arcthis verify archive.tar.gz --json
```

ZIP, 7z, and RAR verification reads every readable file through the underlying integrity checks. TAR and every compressed TAR variant parse each header and read every file through the compression trailer. Single compressed files are decoded completely. Verification checks structural and compression integrity, not cryptographic authenticity or content safety.

## Machine-readable output

Every successful structured document begins with:

```json
{
  "schema_version": "2"
}
```

Archive-based results include:

```json
{
  "archive": {
    "path": "dataset.zip",
    "path_lossy": false,
    "format": "zip"
  }
}
```

Command-specific fields include `entries`, `tree`, `entry`, inspect fields, `find`, `grep`, `hash`, `extraction`, `verification`, `pack`, `index`, and `convert`. Dry-runs use `operation` plus a structured `plan`; batch execution uses `result`. `pack`, `index`, and `convert` use operation-specific outer structures instead of pretending their inputs are ordinary file-query results.

With `--json`, runtime errors are one JSON document on stderr and stdout remains empty:

```json
{
  "schema_version": "2",
  "error": {
    "code": "entry_not_found",
    "message": "archive entry not found: missing.txt",
    "details": { "entry": "missing.txt" }
  }
}
```

The complete JSON format is in [docs/CLI.md](./docs/CLI.md).

## Exit codes

| Exit | Category |
| ---: | --- |
| 0 | Success, including BrokenPipe consumer stop |
| 1 | General I/O error |
| 2 | CLI syntax/usage error from clap |
| 3 | `unsupported_format`, `not_an_archive` |
| 4 | `invalid_archive` or `corrupted_archive` |
| 5 | `entry_not_found` |
| 6 | `permission_denied` |
| 7 | `unsafe_path` |
| 8 | `resource_limit` |
| 9 | `collision` |
| 10 | `unsupported_operation`, password categories |
| 11 | `verification_failed` |
| 12 | `partial_failure` |

## Troubleshooting

### A compressed file exposes an unexpected name

A matching compression suffix is removed (`report.txt.gz` becomes `report.txt`). A stream with a misleading filename uses an `.out` content name so a compression suffix is not silently fabricated.

### ZIP lists but cannot be read or verified

The Rust ZIP implementation supports the methods enabled by its dependency build. A method outside that set returns `unsupported_operation` rather than invoking an external tool.

### Extraction reports `collision`

The destination already exists, the archive contains duplicate/case-colliding paths, a file conflicts with a parent, or two `extract-all` plans resolve to one destination. Keep the default refusal, choose a new `--output`, or explicitly select one collision handling.

### Extraction rejects links

This is intentional. Link restoration requires additional target and ordering rules and is planned only after those rules have regression coverage.

### RAR works but reports limited metadata

The libarchive integration does not expose every RAR property. `inspect` emits `rar_metadata_limited`; read/extract/verify behavior is authoritative. RAR creation and native RAR multi-volume access are not implemented.

### A split archive still fails with `--volume`

Check that the positional path is the first byte segment and every later `--volume` is present in exact order. The feature combines byte-stream splits; it does not reinterpret native volume protocols.

### Where is recursive cross-archive search?

Recursive cross-archive search remains roadmap work after v0.4. Use an explicit `--within` chain for known nested archives; do not rely on undocumented path syntax.
