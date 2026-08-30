# CLI Reference

## Command syntax

The preferred syntax is:

```text
arcthis <command> <archive> [entry] [options]
```

Commands are format-independent. There are no ZIP-specific or TAR-specific top-level commands.

The default build includes the local stdio MCP entry point. `arcthis mcp --allow-root <path>` starts it. This transport is a separate protocol surface rather than a versioned CLI result: stdout contains JSON-RPC only and stderr contains diagnostics. Nine read-only tools are always available. Six extract/pack/convert `_plan`/`_execute` tools are advertised only when an explicit `--allow-output-root` policy exists. Library-only builds can disable it with `--no-default-features`. See [RFC 0003](./RFC-0003-MCP-INTEGRATION.md) for the input/output formats and safety rules.

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

Lifecycle planning rejects source/destination aliases. `pack` also rejects an output inside a directory source. With `--delete-source`, any source/destination ancestor overlap that could remove the saved destination is a `collision`.

## Human and machine output

- Result data goes to stdout.
- Diagnostics, warnings, and progress go to stderr.
- `--json` selects machine output on commands with structured results.
- `read` always emits raw bytes and rejects `--json`.
- JSON and non-TTY output contain no ANSI color decoration.
- `--no-color` and `NO_COLOR` disable ANSI color decoration. Output is intentionally usable without color even in a TTY.
- A stdout BrokenPipe is a successful early consumer termination.

## JSON shape

Every successful structured response is an object with:

```json
{
  "schema_version": "1",
  "archive": {
    "path": "dataset.zip",
    "path_lossy": false,
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
  "kind": "file",
  "size": 1048576,
  "compressed_size": 182341,
  "modified_time": "2026-08-27T12:00:00Z",
  "encrypted": false,
  "executable": false,
  "symlink_target": null,
  "crc32": "c0ffee00",
  "mime_guess": "text/csv"
}
```

Optional or unavailable fields are `null`. `archive_index` preserves source order. `mime_guess` is inferred from the path extension and never reads file content.

`path_encoding` is `utf8` or `escaped_bytes`. Invalid UTF-8 bytes use `%XX` in `path`; a literal percent is `%25` when byte escaping is active. These paths remain queryable, but extraction rejects `escaped_bytes` rather than writing a different filename.

### `list`

Adds `entries`, preserving archive order and duplicates.

### `tree`

Adds `tree`, an array of recursive nodes:

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

Adds `entry`. A missing path returns `entry_not_found`; a duplicate path returns `collision` because selection is ambiguous.

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

Adds `grep` with the literal pattern, optional glob, scan/skip counters, byte count, truncation flag, and matching line objects (`path`, `line_number`, `text`, `line_truncated`). It defaults to a 16 MiB file limit and 10,000 matches, probes the first 8 KiB for NUL bytes, and skips binary files unless `--binary` is set. Individual line retention is capped at 1 MiB.

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
  "schema_version": "1",
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
