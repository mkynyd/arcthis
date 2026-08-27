# RFC 0001: Explicit Nested Archive Traversal

Status: Accepted for v0.3

## Context

Filesystem paths and archive entry paths are distinct namespaces. A locator such as `outer.zip/path/inner.tar/file` cannot reliably identify where filesystem traversal ends and archive traversal begins. Nested access must also avoid silently materializing an inner archive as a temporary filesystem file.

## Decision

The CLI uses a repeatable, explicit `--within <entry>` chain:

```sh
arcthis tree backup.zip --within project.tar.gz --json
arcthis read backup.zip README.md --within project.tar.gz
arcthis read bundle.zip data.csv --within layer.7z --within dataset.tar.zst
```

Each `--within` value names one regular-file entry in the currently open archive. The library streams that entry into a bounded memory source, detects its format from bytes, and opens the next backend through the same format-independent interface. It never creates a named temporary inner archive.

The v0.3 limits are:

- maximum nested depth: 8;
- maximum decoded bytes buffered for each inner archive: 256 MiB by default, configurable with `--max-nested-entry-size`;
- nested extraction and source deletion are unsupported;
- remote locators and persistent nested indexes are outside this RFC.

The final archive reference is rendered as an unambiguous diagnostic path using `::` between traversal steps. This display value is not a locator grammar and must not be parsed as one.

## Library model

Backends receive a repeatable `ArchiveSource` that can reopen either a filesystem file or immutable in-memory bytes. Detection and codecs consume readers from that source. `Archive::open_within` applies the explicit chain and resource limits while the public filesystem `ArchiveLocator` remains stable.

Future locators may add remote/range-readable sources. They must preserve explicit namespace boundaries and cannot reinterpret the diagnostic `::` display string as public syntax.

## Consequences

Nested queries compose with `list`, `tree`, `stat`, `inspect`, `read`, `find`, `grep`, `hash`, and `verify`. An inner archive larger than the configured bound fails with `resource_limit`; unsupported or corrupted inner bytes retain the normal stable error categories. Solid or sequential inner formats continue to report their backend capabilities and costs.
