# Architecture

## Architectural goal

The library presents a small archive interface that hides format-specific enumeration and decoding while preserving meaningful capability and cost differences. The CLI and default-enabled local MCP server are frontends over a shared application service.

## Module model

The design favors deep modules: callers learn a small interface while backend selection, decoder setup, metadata normalization, streaming, limits, and error translation remain local to the archive implementation.

```text
CLI frontend             MCP stdio frontend
       \                 /
        -> typed application service
             -> output/error contracts and finite request limits
  -> Archive interface
       -> format detector
       -> ZIP backend adapter
       -> 7z backend adapter
       -> RAR/libarchive backend adapter
       -> TAR-family backend adapter
       -> single-stream backend adapter
       -> repeatable file/memory/multipart archive sources
       -> query, in-process metadata index, and persistent metadata cache
       -> security and resource-limit enforcement
       -> lifecycle and bounded batch orchestration
       -> transactional extraction/packing/conversion
```

The backend seam is real because ZIP, 7z, TAR-family, and single-stream formats have materially different adapters. It is internal to the library. Backend crate types must not leak through the public interface.

## Application service and MCP frontend

`src/app.rs` is synchronous and frontend-neutral. It accepts typed requests for inspect/list/tree/stat/read/find/grep/hash/verify and returns owned domain results. It centralizes cancellation checkpoints, finite decoded-byte and result budgets, and bounded read windows without depending on Clap, terminal rendering, MCP, or Tokio. The CLI uses a compatibility limit profile so existing JSON and raw-byte behavior remains stable.

`src/mcp.rs` adapts that service to feature-gated stdio MCP using protocol revision `2025-06-18`. It owns transport schemas, tool annotations, canonical input/output-root authorization, cancellation bridging, and UTF-8/base64 window encoding. `src/mcp_mutation.rs` composes the existing extraction, packing, conversion, and lifecycle modules for controlled plan/execute tools. A SHA-256 digest binds the exact request, plan, source fingerprint, destination state, resource limits, collision policy, and deletion intent; execute replans and rejects stale state before mutation.

## Public library interface

The initial `Archive` module owns these operations:

- open an `ArchiveLocator` and detect its format from content;
- enumerate normalized entry metadata;
- look up one entry;
- inspect archive-level information and capabilities;
- copy one entry to a caller-provided `Write` stream;
- verify all readable entry data;
- safely extract through an explicit extraction plan.
- find entry paths and stream content into grep/hash consumers;
- traverse an explicit nested-entry chain through bounded in-memory sources.

The interface intentionally prefers `copy_entry_to` over returning a borrowed decoder. ZIP entry readers often borrow archive state, while TAR readers are sequential. A caller-provided writer gives both adapters one streaming interface without self-referential types, whole-entry buffering, or backend leakage.

## Locator model

`ArchiveLocator` remains the public explicit filesystem source. Internally, each backend receives a repeatable `ArchiveSource` that reopens a filesystem file, immutable in-memory bytes, or an explicitly ordered multipart byte stream. The multipart reader implements `Read + Seek` across file boundaries without concatenating all volumes into memory. Format detection and codecs consume readers from the same source seam.

Nested traversal never uses ambiguous path concatenation such as `outer.zip/inner.tar/file`. A repeatable ordered `--within` chain streams each selected outer entry into a bounded memory source and opens it through the normal detector/backend path. The accepted semantics and limits are recorded in [RFC 0001](./RFC-0001-NESTED-ARCHIVES.md).

Multipart traversal is independent from nested traversal. The positional file is the first byte segment and repeatable `--volume` paths append exact later segments. This supports byte-split streams while explicitly excluding format-native RAR volume protocols; see [RFC 0002](./RFC-0002-MULTIPART-SOURCES.md).

## Format detection

Detection is content-first:

1. Read the required prefix/header bytes.
2. Match ZIP, 7z, RAR/RAR5, Gzip, Bzip2, XZ, or Zstandard signatures and validate the selected decoder.
3. For compressed streams, inspect the decoded prefix and distinguish TAR containers from one-payload streams.
4. Validate TAR header structure/checksum rather than relying only on `.tar`.
5. Use extensions only for output format selection during `pack`, or as diagnostic context.

A misleading input extension does not override valid magic bytes. A non-TAR compressed stream is exposed as one implicit entry derived from its filename. Because these formats have no entry table, size enumeration requires a sequential decode and `inspect` reports that cost.

## Normalized entry model

An entry has a stable archive index, normalized archive path, and metadata that can be obtained without content-wide analysis: path encoding, kind, uncompressed size, optional compressed size, optional modified time, encryption indication when available, executable indication, optional link target, optional integrity checksum, and an optional extension-based MIME guess.

Non-UTF-8 entry bytes are exposed with deterministic `%XX` escaping and `path_encoding: escaped_bytes`. Query commands can address the displayed value. Extraction refuses these names rather than silently creating a different filesystem path.

The model does not sniff MIME from content and does not compute a content hash during enumeration. Unsupported or unavailable metadata is represented explicitly with nullable fields, not fabricated values.

Duplicate archive paths remain distinct during enumeration. Operations that name a single path reject ambiguity rather than silently selecting an entry.

## Capability model

Capabilities describe semantics and likely access cost:

- `random_access`: selected entries can be reached without sequentially decoding earlier content;
- `streaming_read`: entry bytes can be copied to a writer without whole-entry buffering;
- `encrypted`: the backend can read encrypted entries;
- `solid`: the format/archive uses solid compression;
- `can_create`, `can_extract`, `can_verify`, `can_seek`.

ZIP reports random access and seek capability. 7z reports block/solid-dependent access. RAR, TAR-family, and single-stream formats report sequential access. RAR advertises read/extract/verify but not creation, and emits a metadata-limited warning because libarchive does not expose every RAR property through the adapter. These flags describe the implemented adapter, not only theoretical format capability.

## Streaming model

Content operations use bounded buffers and `Read`/`Write` streaming. They must not call `read_to_end` for untrusted large entries. Compressed TAR and solid 7z may decode preceding data to reach a selected entry; `inspect` exposes this limitation through capabilities and warnings.

Enumeration builds one in-process metadata index per open `Archive`, avoiding repeated sequential enumeration when one command performs filtering followed by named access. Returned public snapshots remain owned values so backend lifetimes do not leak.

`index` can persist the same normalized metadata under the platform cache root. The cache key hashes the canonical archive path and the document fingerprints source size plus nanosecond modification time. Documents are transactionally replaced and ignored when malformed, schema-mismatched, format-mismatched, or stale. Persistent indexes are metadata caches, not decoded content or TAR seek-point indexes.

`find` filters only this index. `hash` streams one entry into a digest writer. `grep` filters metadata before opening content, enforces size/match/line bounds, performs a bounded binary probe, and streams decoded bytes into a line scanner without materializing files.

## Extraction model

Full extraction is plan-driven:

1. enumerate and validate every entry path and declared size;
2. reject links, special files, duplicate/case-insensitive collisions, non-UTF-8 names, unsafe paths, and resource-limit violations;
3. choose the explicit or intelligent destination;
4. extract into a temporary sibling staging location;
5. close and flush all files;
6. commit the completed staging tree to the destination;
7. optionally delete the source only after commit succeeds.

An existing destination is refused by default. Explicit overwrite moves the prior destination to a sibling backup, commits the staged replacement, restores on commit failure, and removes the backup only after success. Skip never deletes the source; rename resolves a new numbered destination before writing.

Lifecycle planning canonicalizes existing paths and resolves missing destinations through their nearest existing ancestor. Source and destination aliases are rejected. A pack destination cannot be inside a directory source, and source deletion is rejected whenever either path contains the other. Selected-entry extraction verifies the complete archive before commit when source deletion is requested. The accepted invariants and tradeoffs are recorded in [ADR 0001](./ADR-0001-TRANSACTIONAL-LIFECYCLE.md).

Single-entry extraction writes a temporary sibling file and commits it only after the stream completes. Directory entries are not accepted as single-file output targets.

Cross-filesystem rename fallback is deliberately not needed when staging is created beside the destination. Future remote or user-selected staging locations must define a verified copy fallback before use.

## Intelligent destination rule

For `arcthis extract archive.ext`:

- if all entries are under one real top-level directory, that directory becomes the final destination and the common prefix is stripped during staging;
- otherwise, the final destination is a directory named from the full archive stem (`backup.tar.gz` -> `backup`), and entry paths remain unchanged;
- an explicit `--output` is always the extraction root and does not trigger prefix stripping;
- destination collisions follow the explicit collision policy and default to refusal.

## Transactional packing

Packing determines the output format from the requested output suffix, rejects source links/special files, writes to a temporary sibling file, finalizes and syncs the encoder, reopens and verifies the result through the normal `Archive` interface, and only then applies the collision policy and commits. Source deletion occurs strictly after commit.

## Verified conversion

v0.4 conversion deliberately composes existing deep modules. It opens and validates the source through `Archive`, materializes safe entries into a system temporary directory under extraction limits, invokes packing without adding the staging root name, reopens and verifies the staged target, commits through the shared collision lifecycle, and only then may delete one source archive. This is not a claim of direct format-to-format streaming; the JSON plan exposes `staged_materialization`.

Single-stream targets require one root-level regular entry. Nested conversion and multipart source deletion are rejected. Target creation never invokes a RAR writer.

## Batch orchestration

`extract-all` discovers supported archives by opening content rather than trusting extensions. It completes per-archive planning before execution, rejects duplicate planned destinations, and processes independent archives with a bounded synchronous worker pool. Each worker calls the same `Archive::extract` path used by the single-archive command. Results are sorted for deterministic machine output; partial failures use the stable `partial_failure` category.

## Error model

The library returns typed errors with stable public categories. Backend errors are translated at the seam. The CLI maps categories to structured machine errors and process exit codes. Internal crate names, debug representations, and backtraces are not a public interface.

## Future evolution

- Additional backend adapters can join the internal seam without changing command implementations.
- Seek-point and range indexes may extend the existing metadata cache when measurements justify them.
- Remote locators can reuse the source seam once range and retry semantics are specified.
- HTTP, FFI, and language bindings can reuse the application service; MCP already does so.
- Async I/O is added only if a server/remote-source performance model demonstrates value.

The staged integration plan is documented in [the v0.5+ integrations plan](./V0.5-INTEGRATIONS-PLAN.md).
