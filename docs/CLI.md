# CLI Contract

## Grammar

The preferred grammar is:

```text
arcthis <command> <archive> [entry] [options]
```

Commands are format-independent. There are no ZIP-specific or TAR-specific top-level commands.

## v0.1 commands

```text
arcthis list <archive>
arcthis tree <archive>
arcthis stat <archive> <entry>
arcthis inspect <archive>
arcthis read <archive> <entry>
arcthis extract <archive> [entry] [--output <path>]
arcthis pack <source> --output <archive>
arcthis verify <archive>
```

`read` writes raw entry bytes and is intentionally not a JSON operation. It is the primitive for pipelines such as:

```sh
arcthis read source.tar.gz src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

`head` is not a v0.1 command because `read | head` already provides composable behavior.

## Human and machine output

- Result data goes to stdout.
- Diagnostics, warnings, and progress go to stderr.
- `--json` selects machine output on commands with structured results.
- `read` always emits raw bytes and rejects `--json`.
- JSON and non-TTY output contain no ANSI decoration.
- `--no-color` and `NO_COLOR` disable ANSI decoration. v0.1 output is intentionally usable without color even in a TTY.
- A stdout BrokenPipe is a successful early consumer termination.

## JSON envelope

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

Command-specific result fields are added to this envelope. JSON is emitted as one complete document followed by a newline.

### Entry schema

```json
{
  "path": "train/data.csv",
  "path_encoding": "utf8",
  "kind": "file",
  "size": 1048576,
  "compressed_size": 182341,
  "modified_time": "2026-08-27T12:00:00Z",
  "encrypted": false,
  "executable": false,
  "symlink_target": null,
  "crc32": "c0ffee00"
}
```

Optional or unavailable fields are `null`; fields are not populated through expensive MIME/content scanning.

`path_encoding` is `utf8` or `escaped_bytes`. Invalid UTF-8 bytes use `%XX` in `path`; a literal percent is `%25` when byte escaping is active. These paths remain queryable, but v0.1 extraction rejects `escaped_bytes` rather than materializing a different filename.

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

- `compression`, `encrypted`, `solid`, `random_access`;
- `entry_count`, `compressed_size`, `uncompressed_size`, `compression_ratio`;
- `warnings` as stable objects with `code` and `message`;
- `capabilities` as booleans.

Inspection avoids entry-content scanning beyond what the format requires for enumeration.

### `verify`

Adds a `verification` object with `verified`, `entries_checked`, and `bytes_checked`.

### `extract`

Adds an `extraction` object with `destination`, `entries_extracted`, and `bytes_written`. Full extraction accepts `--max-entries`, `--max-total-size`, and `--max-entry-size`. A selected entry requires `--output <file>`.

### `pack`

Adds a `pack` object with `source`, `destination`, `format`, `entries_packed`, `archive_size`, and nested `verification`. Because the input is a filesystem source, this response has no input `archive` envelope.

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

The following are not v0.1 commands or options: `find`, `grep`, `hash`, `extract-all`, `convert`, `preview`, nested `--within`, `--dry-run`, `--delete-source`, `--overwrite`, `--skip-existing`, and `--rename`.

Their eventual introduction requires matching library support, JSON schemas, tests, and documentation. `--delete-source` will be one shared lifecycle option for pack, extract, and convert after transactional dry-run semantics exist.
