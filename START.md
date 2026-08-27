# Using arcthis

[简体中文](./START.zh-CN.md)

This guide describes the CLI that is implemented in v0.1. For product goals and planned commands, see [docs/PRODUCT.md](./docs/PRODUCT.md) and [ROADMAP.md](./ROADMAP.md).

## Build and install

The repository pins Rust 1.98.0.

```sh
cargo build --release --locked
./target/release/arcthis --version
```

Install the current checkout with:

```sh
cargo install --path . --locked
```

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

Input formats are detected from content. ZIP supports Stored and Deflate content access in the current build. TAR and TAR.GZ are sequential formats, so reading one late entry may scan earlier archive data. `inspect` reports this as `random_access: false` and emits a `sequential_access` warning.

## Command shape and global options

```text
arcthis <command> <archive> [entry] [options]
```

Global options can appear with a subcommand:

- `--json` emits a schema-versioned machine result where supported.
- `--no-color` disables terminal color decoration. v0.1 output does not currently require color.
- `-h`, `--help` shows command help.
- `-V`, `--version` shows the version.

`NO_COLOR`, non-TTY output, and JSON output contain no ANSI decoration.

## `inspect` — learn archive cost and risk

```sh
arcthis inspect archive.tar.gz
arcthis inspect archive.tar.gz --json
```

`inspect` enumerates archive metadata without reading every file's content. It reports format, compression, entry count, declared sizes, an approximate archive-size ratio, capabilities, and warnings.

Important warning codes include:

- `sequential_access` — selected reads may scan from the beginning.
- `encrypted_entries_unsupported` — encrypted content is present but unsupported.
- `non_regular_entries` — links or special entries will be rejected by extraction.
- `duplicate_entry_paths` — named access is ambiguous and extraction will refuse the archive.
- `unsafe_entry_paths` — extraction path validation would reject at least one entry.
- `default_extraction_limits_exceeded` — declared metadata exceeds v0.1 defaults.

Inspection warnings inform planning. The extraction path independently enforces the corresponding checks.

## `list` — enumerate entries

```sh
arcthis list archive.zip
arcthis list archive.zip --json
```

Human output is a tab-separated `KIND`, `SIZE`, and `PATH` table. JSON preserves archive order and duplicate entries.

An entry object includes:

- `path` and `path_encoding` (`utf8` or `escaped_bytes`);
- `kind`: `file`, `directory`, `symlink`, `hardlink`, or `other`;
- `size` and optional `compressed_size`;
- optional `modified_time`;
- `encrypted`, `executable`, optional `symlink_target`, and optional `crc32`.

Invalid UTF-8 entry bytes are represented with `%XX` escaping and `path_encoding: "escaped_bytes"`. They can be listed and addressed by the displayed value. v0.1 refuses to materialize them during extraction because it cannot preserve the original filesystem name unambiguously.

## `tree` — view the logical file tree

```sh
arcthis tree source.tar
arcthis tree source.tar --json
```

Human output uses tree characters. JSON returns recursive nodes with `name`, logical `path`, `kind`, an optional source `entry`, and `children`. Implicit directories have `entry: null`; duplicate file leaves remain separate.

## `stat` — inspect one named entry

```sh
arcthis stat archive.zip README.md
arcthis stat archive.zip README.md --json
```

The path must match the entry path shown by `list`. A missing path returns `entry_not_found`. If the archive contains the same path more than once, `stat` returns `collision` instead of silently choosing one.

## `read` — stream one entry

```sh
arcthis read archive.zip README.md
arcthis read source.tar.gz src/lib.rs | rg unsafe
arcthis read media.zip video.mp4 | ffprobe -i pipe:0
```

`read` is the core content primitive. It writes only raw entry bytes to stdout and writes diagnostics to stderr. It never wraps bytes in JSON and therefore rejects `--json` with `unsupported_operation`.

Regular files are supported. Directories, links, and special entries are rejected for `read`. BrokenPipe exits successfully, so `arcthis read ... | head` is a normal workflow.

For TAR and TAR.GZ, v0.1 first checks that the selected path is unique, then performs a sequential content scan. This may decode the stream more than once; it does not materialize the archive on disk.

## `extract` — safely materialize content

### Extract all entries

```sh
arcthis extract archive.zip
arcthis extract archive.tar.gz --output ./restored
```

An explicit `--output` is the complete extraction root. Without it:

- if every entry is under one real top-level directory, that directory is committed directly;
- otherwise, a directory is derived from the complete archive suffix (`backup.tar.gz` becomes `backup/`);
- the destination must not already exist.

Examples:

```text
archive: project/README.md, project/src/lib.rs
result:  ./project/README.md, ./project/src/lib.rs

archive: README.md, src/lib.rs
input:   bundle.tar.gz
result:  ./bundle/README.md, ./bundle/src/lib.rs
```

### Extract one regular file

Single-entry extraction requires an explicit output file:

```sh
arcthis extract archive.zip README.md --output ./README.md
```

The command writes a temporary sibling file, flushes and syncs it, and then uses a no-clobber commit. Directory or link entries are unsupported.

### Resource limits

```sh
arcthis extract archive.zip \
  --max-entries 50000 \
  --max-total-size 8589934592 \
  --max-entry-size 2147483648
```

Values are raw bytes/counts. Defaults are:

| Option | Default |
| --- | ---: |
| `--max-entries` | 100,000 |
| `--max-total-size` | 16 GiB |
| `--max-entry-size` | 4 GiB |

Declared metadata is checked before staging. Actual bytes are counted while streaming. A violation returns `resource_limit` and does not commit the destination.

### Extraction safety

v0.1 rejects absolute paths, `.`/`..`, backslashes, Windows drive/UNC prefixes, NULs, duplicate paths, case-insensitive collisions, file-as-parent conflicts, overlong paths, invalid UTF-8 names, symlinks, hardlinks, and special files. It stages on the destination filesystem and commits with rename only after every planned entry succeeds.

There is no `--overwrite`, `--skip-existing`, or `--rename` in v0.1. See [docs/SECURITY.md](./docs/SECURITY.md).

## `pack` — create and verify an archive

```sh
arcthis pack ./project --output project.zip
arcthis pack ./project --output project.tar
arcthis pack ./project --output project.tar.gz --json
```

The output suffix selects ZIP, TAR, or TAR.GZ. Directory packing includes the source directory itself as the top-level entry, preserving empty directories and regular files. ZIP output uses Deflate.

Symlinks and special files are rejected. The destination must not exist. The lifecycle is:

```text
scan source -> write temporary sibling -> finalize -> sync -> reopen -> verify -> no-clobber commit
```

The source is never deleted. `--delete-source` is planned, not implemented.

## `verify` — check readable archive data

```sh
arcthis verify archive.zip
arcthis verify archive.tar.gz --json
```

ZIP verification opens and streams every entry so CRC validation runs. TAR/TAR.GZ verification parses each header and streams every entry; the Gzip stream is consumed through its integrity trailer. Verification checks structural and codec integrity, not cryptographic authenticity or content safety.

## Machine output

Every successful structured document begins with:

```json
{
  "schema_version": "1"
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

Command-specific fields are `entries`, `tree`, `entry`, inspect fields, `extraction`, `verification`, or `pack`. `pack` has no input archive envelope because its input is a filesystem source.

With `--json`, runtime errors are one JSON document on stderr and stdout remains empty:

```json
{
  "schema_version": "1",
  "error": {
    "code": "entry_not_found",
    "message": "archive entry not found: missing.txt",
    "details": { "entry": "missing.txt" }
  }
}
```

The complete schema contract is in [docs/CLI.md](./docs/CLI.md).

## Exit codes

| Exit | Category |
| ---: | --- |
| 0 | Success, including BrokenPipe consumer stop |
| 1 | General I/O error |
| 2 | CLI syntax/usage error from clap |
| 3 | `unsupported_format` |
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

### `unsupported_format` for a `.gz` file

v0.1 supports TAR.GZ, not arbitrary single-file Gzip streams. Detection requires decompressed TAR structure.

### ZIP lists but cannot be read or verified

The current build supports Stored and Deflate entry methods. Another ZIP compression method returns `unsupported_operation`.

### Extraction reports `collision`

The destination already exists, the archive contains duplicate/case-colliding paths, or an entry conflicts with a parent. v0.1 never overwrites. Choose a new `--output` path or inspect the archive.

### Extraction rejects links

This is intentional. Link restoration requires additional target and ordering rules and is planned only after those rules have regression coverage.

### Where are `find`, `grep`, `hash`, `extract-all`, `convert`, `--dry-run`, and `--delete-source`?

They are roadmap capabilities. Compose v0.1 `read` with existing tools when possible; do not rely on undocumented placeholders.
