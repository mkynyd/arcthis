# RFC 0002: Multipart Byte-Stream Sources

Status: Accepted for v0.4

## Context

Some archives are distributed as multiple files — either because they were split at arbitrary byte boundaries or because a format defines its own volume protocol. The two cases are different, and conflating them produces archives that fail content detection or corruption.

`arcthis` needs a way to open an archive whose bytes span several files without claiming to understand every format's native volume grammar.

## Decision

The CLI exposes a repeatable `--volume <path>` option:

```sh
arcthis list archive.7z.001 --volume archive.7z.002 --volume archive.7z.003 --json
arcthis read archive.7z.001 payload.txt --volume archive.7z.002 --volume archive.7z.003
```

Semantics:

1. The positional archive path is the **first** byte segment.
2. Each `--volume` path appends one **exact** later byte segment in the order supplied.
3. The combined stream is opened as a seekable, repeatable `ArchiveSource` and passed to normal content-first detection and backend semantics.
4. Paths must be unique and every segment must exist; otherwise the open fails with a stable error.

`inspect` reports `multipart: true` and `volume_count` when volumes were supplied. The combined source supports `list`, `tree`, `stat`, `inspect`, `read`, `find`, `grep`, `hash`, and `verify`; `extract` is supported without source deletion.

## What this is not

This is a **byte-stream concatenation** model. It covers archives split at arbitrary boundaries — for example a split 7z stream. It does **not** implement format-native RAR multi-volume sets (`.part1.rar`/`.r00`), which are not simple concatenations and carry per-volume headers and indexes. Format-native RAR volume traversal remains outside this RFC; see [docs/RAR.md](./RAR.md).

## Consequences

- `--delete-source` is rejected for multipart `extract` and `convert`: the single-source lifecycle guarantee cannot be applied atomically across several volume files, so the sources are preserved.
- `extract-all` and `index` reject `--volume`; they operate on one filesystem archive source each.
- The volume contract is additive and does not change the behavior of single-file archives.

## Library model

The `ArchiveSource` abstraction already reopens either a filesystem file or immutable in-memory bytes. Multipart sources extend this with an ordered vector of segment paths. Detection and codecs consume readers from the combined source without knowing how many files contributed bytes. This keeps the backend seam format-independent and lets future locators reuse the same rule: each source must be seekable and repeatable.
