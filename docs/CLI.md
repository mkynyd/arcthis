# CLI Reference

## Command syntax

This document follows unreleased **0.6.0-rc.1 / schema 2**. Public 0.5.1 remains schema 1. The breaking metadata/envelope changes and migration are recorded in [ADR 0002](./ADR-0002-EXPLORATION-CONTRACT.md); complete report acceptance is in [EXPLORATION-ITERATION-PLAN.md](./EXPLORATION-ITERATION-PLAN.md).

The preferred syntax is:

```text
arcthis <command> <archive> [entry] [options]
```

Commands are format-independent. There are no ZIP-specific or TAR-specific top-level commands.

The default build includes the local stdio MCP entry point. `arcthis mcp --allow-root <path>` starts it. This transport is a separate protocol surface rather than a versioned CLI result: stdout contains JSON-RPC only and stderr contains diagnostics. Nine read-only tools are always available. Six extract/pack/convert `_plan`/`_execute` tools are advertised only when an explicit `--allow-output-root` policy exists. Library-only builds can disable it with `--no-default-features`. See [RFC 0003](./RFC-0003-MCP-INTEGRATION.md) for the input/output formats and safety rules.

The MCP server defaults to `--max-concurrent-requests 4` (allowed range 1–64) and `--max-response-bytes 16777216`. Per-request ceilings default to `--max-entries 100000`, `--max-decoded-bytes 1073741824`, `--max-results 10000` find/grep results, and `--max-read-window 1048576` bytes for one `archive_read` window. Excess concurrent calls return `resource_limit`; clients may retry after an active request completes. The response budget counts both structured and text JSON, including escaping, with 256 bytes reserved for the tool-result envelope. It must be at least 1024 bytes for read-only servers or 131072 bytes when output roots are enabled. Oversized results return an error, not a partial list/tree. These limits apply to tool results, not discovery metadata or the outer JSON-RPC envelope.

## Root help (current checkout)

Root help begins with a plain ASCII rendition of the A2 mark and the `arcthis` wordmark. `arcthis` with no arguments writes this help to stderr and retains exit code 2; `arcthis -h`, `arcthis --help`, and `arcthis help` write root help to stdout and exit 0. The banner is absent from subcommand help, `--version`, normal command results, runtime JSON errors, and MCP protocol output. It has no ANSI escapes, including non-TTY and `NO_COLOR`/`--no-color` use. This checkout addition has not been released in the v0.5.1 binaries.

## Implemented commands

```text
arcthis list <archive>
arcthis tree <archive>
arcthis stat <archive> <entry>
arcthis inspect <archive>
arcthis read <archive> <entry>
arcthis find <archive> --glob <pattern>
arcthis grep <archive> <literal-pattern> [--glob <pattern>]
arcthis hash <archive> <entry> [--algorithm sha256|sha512]
arcthis index <archive> [--refresh|--delete] [--dry-run]
arcthis extract <archive> [entry] [--output <path>]
arcthis extract-all <directory> [--recursive] [--workers <count>]
arcthis pack <source> --output <archive>
arcthis verify <archive>
arcthis convert <archive> --output <archive>
```

Archive access commands accept a repeatable `--within <entry>` chain and `--max-nested-entry-size <bytes>`. `extract` and `convert` explicitly reject nested traversal in v0.5.

Archive-opening commands accept `--password-file <path>`, repeatable ordered `--volume <path>`, and `--index-directory <path>`. `pack` rejects password, volume, and nested-source options because encrypted creation is not implemented. `extract-all` rejects nested and split sources. Split extraction/conversion rejects `--delete-source`.

`read` writes raw file bytes and is intentionally not a JSON operation. It is the building block for pipelines such as:

```sh
arcthis read source.tar.gz src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

`head` is not a command because `read | head` already provides composable behavior.

`extract`, `pack`, and `convert` accept `--dry-run`, `--delete-source`, and one of `--overwrite`, `--skip-existing`, or `--rename`. `extract-all` accepts the same lifecycle flags plus capped workers and optional filesystem recursion. `index` has its own create/refresh/delete dry-run lifecycle.

Lifecycle planning rejects source/destination aliases and ancestor/descendant overlap for every write, even without `--delete-source`. Recursive `extract-all` also rejects a planned destination that overlaps another destination or discovered archive source.

## Human and machine output

- Result data goes to stdout.
- Diagnostics, warnings, and progress go to stderr.
- `--json` selects machine output on commands with structured results.
- `read` always emits raw bytes and rejects `--json`.
- JSON and non-TTY output contain no ANSI color decoration.
- `--no-color` and `NO_COLOR` disable ANSI color decoration. Output is intentionally usable without color even in a TTY.
- A stdout BrokenPipe is a successful early consumer termination.

## JSON shape

Every successful structured response carries `schema_version`. Archive access responses also carry this reference (index and filesystem-only operations have their own result objects):

```json
{
  "schema_version": "2",
  "archive": {
    "path": "/data/dataset.zip",
    "path_lossy": false,
    "index_used": false,
    "format": "zip"
  }
}
```

Command-specific result fields are added to this object. JSON is emitted as one complete document followed by a newline.

### File format

```json
{
  "archive_index": 42,
  "path": "train/data.csv",
  "path_encoding": "utf8",
  "path_raw_hex": "747261696e2f646174612e637376",
  "kind": "file",
  "size": 1048576,
  "compressed_size": 182341,
  "modified_time": "2026-08-27T12:00:00Z",
  "modified_time_local": null,
  "encrypted": false,
  "executable": false,
  "symlink_target": null,
  "crc32": "c0ffee00",
  "mime_guess": "text/csv"
}
```

Optional or unavailable fields are `null`. `archive_index` preserves source order. `mime_guess` is inferred from the path extension and never reads file content.

`path_raw_hex` is the filename bytes exposed by the adapter: ZIP/TAR/Gzip byte names, or UTF-8 for Unicode names exposed by 7z/RAR libraries. `path_encoding` is utf8, escaped_bytes, cp437, gbk or shift_jis. Escaped bytes use `%XX` (literal percent becomes `%25` in that mode); display strings may coincide with literal UTF-8 names, so ambiguous name selection returns collision. `--entry-index` distinguishes instances. Explicit `--encoding` uses strict conversion and applies path/collision checks to the decoded name. Directory executable and CRC are null; unavailable/shared compressed sizes are null. `modified_time` is absolute RFC3339 or null; ZIP DOS timestamps with unknown timezone are in `modified_time_local`.

### `list`

Adds `entries`, preserving archive order and all repeated instances, including ZIP. `archive_index` is an instance identifier; name selection rejects ambiguity, while `--entry-index` selects one regular entry for stat/read/hash/extract.

### `tree`

Adds `tree`, an array of recursive nodes:

Tree paths are limited to 4096 bytes and 256 non-empty components. Larger paths return `resource_limit` instead of constructing an unbounded recursive result.

```json
{
  "name": "train",
  "path": "train",
  "kind": "directory",
  "entry": null,
  "children": []
}
```

Implicit directories have `entry: null`. Duplicate leaves remain representable.

### `stat`

Adds `entry`. A missing path returns `entry_not_found`; a repeated display path returns `collision`. `--entry-index N` disambiguates. `stat --verbose` adds raw-name bytes, path encoding, executable/encryption/CRC/MIME facts and unknown-timezone local time to human output.

### `inspect`

Adds:

- `compression`, `encrypted`, `solid`, `random_access`, `multipart`, `volume_count`;
- `entry_count`, `compressed_size`, `uncompressed_size`, `compression_ratio`;
- `warnings` as stable objects with `code` and `message`;
- `capabilities` as booleans.

Inspection avoids reading file content beyond what the format requires for listing. Single-file formats have no file table, so determining their one content size requires sequential decoding and emits `single_stream_metadata_scan`.

### `find`

Adds `find` with the requested `glob`, `matched` count, and complete matching file objects. Glob matching applies to full normalized archive file paths and never reads file content.

### `grep`

ZIP reuses one central directory; TAR-family, 7z and RAR reuse one content traversal. Entry filtering precedes selected content copying. This works for normal, nested-memory and explicit byte-split sources; sequential codecs may still decode preceding bytes. The same versioned application result is serialized by CLI and MCP.

Adds `grep` with the literal pattern, optional glob, scan/skip counters, byte count, truncation flag, and matching line objects (`path`, `line_number`, `text`, `line_truncated`). It defaults to a 16 MiB file limit and 10,000 matches, probes the first 8 KiB for NUL bytes, and skips binary files unless `--binary` is set. Individual line retention is capped at 1 MiB.

`--max-result-bytes` defaults to 16777216 and counts serialized matching-line objects plus a comma allowance across all files, including paths, JSON escaping, and UTF-8 replacement bytes. A match that does not fit is omitted as a whole and sets the existing `matches_truncated` flag. Zero retains no matches. MCP accepts the same `max_result_bytes` input but clamps it to at most one quarter of the server response budget (and the service result ceiling), leaving room for both JSON copies and metadata. Actual grep scan bytes across all files also share the MCP decoded-byte ceiling; exceeding it returns `resource_limit`.

### `hash`

Adds `hash` with `entry`, `algorithm`, lowercase hexadecimal `digest`, and `bytes_hashed`. SHA-256 is the default; SHA-512 is also implemented. File bytes are read into the digest.

### Password input

`--password-file` reads secret bytes and strips trailing CR/LF. Passwords are never accepted as ordinary CLI values. ZIP and 7z map missing and incorrect credentials to `password_required` and `wrong_password`. RAR behavior depends on the implementation. Passwords are redacted from library `Debug` output.

### Split `--volume`

The positional archive is volume one. Each repeatable `--volume` path appends one exact byte segment. The combined source is seekable and repeatable, then uses normal signature detection and normal behavior. `inspect` exposes `multipart` and `volume_count`. This covers byte-stream splits, not native RAR volume protocols; see [RFC 0002](./RFC-0002-MULTIPART-SOURCES.md).

### `index`

Adds `operation: "index"` and an `index` object with `archive`, `index_path`, `action`, `entries_indexed`, and `dry_run`. Stable actions are `created`, `refreshed`, `reused`, `deleted`, `would_create`, `would_refresh`, `would_reuse`, `would_delete`, and `missing`. Cache documents have their own internal format and are not the public CLI format.

### Nested `--within`

Each `--within` value names one regular-file entry in the current archive. The decoded bytes become a capped read-only in-memory source for the next archive. Depth is capped at 8 and each inner file defaults to 256 MiB maximum. The final diagnostic archive path uses `::`, but that display string is not a public path syntax. See [RFC 0001](./RFC-0001-NESTED-ARCHIVES.md).

### `verify`

Adds a `verification` object with `verified`, `entries_checked`, and `bytes_checked`.

### `extract`

Adds an `extraction` object with `destination`, `entries_extracted`, `bytes_written`, `status`, and `source_deleted`. Full extraction accepts file/size limits plus optional `--max-compression-ratio` and `--max-entry-duration-seconds`. A selected file requires `--output <file>`.

When selected-file extraction uses `--delete-source`, the complete archive is verified before save and deletion; a corrupt unselected file therefore fails with `verification_failed` and preserves both source and final destination state.

With `--dry-run`, `extract` returns `operation: "extract"` and a structured `plan` containing the resolved destination, estimated size, collision facts, warnings, and delete-source intent.

### `extract-all`

Dry-run returns `operation: "extract_all"` and a plan containing one extraction plan per discovered archive plus cross-plan destination conflicts. Execution returns deterministic sorted items and aggregate `discovered`, `succeeded`, `skipped`, and `failed` counts. If any item fails, stdout still contains the structured result and stderr receives `partial_failure`.

### `pack`

Adds a `pack` object with `source`, `destination`, `format`, `entries_packed`, `archive_size`, nested `verification`, `status`, and `source_deleted`. Because the input is a filesystem source, this response has no input `archive` object. `pack --dry-run` returns `operation: "pack"` and a structured plan without creating an archive.

### `convert`

Adds a `convert` object with source/destination paths, source/target formats, converted file count, output size, nested verification, operation status, and source-deletion result. `convert --dry-run` returns `operation: "convert"` plus a plan containing `access_strategy: "staged_materialization"`, validated source counts/sizes, collision resolution, and delete intent. Conversion preserves archive file paths and uses extraction resource/path validation before any extraction.

## Machine errors

With `--json`, command errors are written to stderr as:

```json
{
  "schema_version": "2",
  "error": {
    "code": "entry_not_found",
    "message": "archive entry was not found",
    "details": {
      "entry": "README.md"
    }
  }
}
```

stdout remains empty. Initial categories are:

- `invalid_argument`
- `not_an_archive`
- `unsupported_format`
- `invalid_archive`
- `corrupted_archive`
- `entry_not_found`
- `permission_denied`
- `unsafe_path`
- `resource_limit`
- `password_required`
- `wrong_password`
- `collision`
- `unsupported_operation`
- `verification_failed`
- `partial_failure`
- `io_error`

Clap syntax errors use exit code 2. The stable runtime category mapping is documented in `START.md`.

## Planned CLI work

`preview`, recursive cross-archive search, format-native RAR multipart traversal, password prompting/secret-provider integration, and remote sources remain planned. They require matching library support, JSON formats, tests, and documentation before becoming public behavior.

## Source-checkout options (0.6.0-rc.1)

- `--entry-index N`: stat/read/hash/extract select an exact `archive_index`; within chains apply it only at the final archive. Omitting entry is supported for stat/read/hash if selection is unambiguous.
- `--encoding utf8|cp437|gbk|shift-jis`: strict explicit name decoding, retaining `path_raw_hex`. Default remains UTF-8 or escaped bytes; index maintenance stores original names and rejects decoding overrides.
- `read --verify-first [--max-entry-size N]`: validate before stdout, with anonymous temporary storage and a 4 GiB default actual-byte limit. Default streaming can produce partial output before error; check exit status. `read --json` is invalid_argument/2 before reading archive/password/stdin data.
- `--no-sync`: extraction, batch extraction and conversion skip per-file fsync, retaining staging/rollback/verification/source-deletion rules. Default syncing remains enabled.
- `extract-all --continue-on-error`: planning/discovery failures and cross-plan conflicts become sorted per-archive failures; independent archives run. Dry-run exposes `plan.errors` and returns partial_failure/12 if nonempty. Default planning remains fail-fast.
- `pack --strip-root`: pack a directory's contents. Output permissions follow umask. Gzip pack writes FNAME/mtime; reads prefer FNAME, then strip known compression and common split suffixes from source names.
- `index --list` / `index --prune [--dry-run] [--max-age-seconds N]`: list or remove only stale/invalid/missing-source/expired hash-named index documents. Default age is 30 days; zero expires all matching documents. `archive.index_used` reports persistent-cache hits. Unix ctime/dev/inode supplements size/mtime; this is not a cryptographic source fingerprint.
- Read-only commands accept source `-`. Compressed stdin buffering defaults to 64 MiB (`--max-stdin-bytes`), with `--stdin-name` for naming or empty compressed-TAR suffixes. No volume mixing or destructive stdin workflows.
- `stat --verbose` prints extra metadata. Human inspect includes capabilities. Source deletion gets an explicit receipt. Lifecycle references and plan/result filesystem paths are canonical absolute paths; stdin/nested references remain explicit virtual names.
- MCP uses the same read-only success structures and schema version as CLI. Output schemas allow success/error envelopes, preserving root `$defs` references. Errors also expose structuredContent and stable details; parameter failures are normalized. Per-request password_file is root-authorized and capped at 4096 bytes for read-only tools. `mcp --json` is invalid; clean stdin EOF exits zero.

Run `arcthis --help` for the full process exit-code table. `NO_COLOR` and `--no-color` remain compatible idempotent options because output is currently plain text.
