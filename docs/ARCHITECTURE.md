# Architecture

## Architectural goal

The library presents a small archive interface that hides format-specific enumeration and decoding while preserving meaningful capability and cost differences. The CLI is one frontend over that interface.

## Module model

The design favors deep modules: callers learn a small interface while backend selection, decoder setup, metadata normalization, streaming, limits, and error translation remain local to the archive implementation.

```text
CLI frontend
  -> output/error contracts
  -> Archive interface
       -> format detector
       -> ZIP backend adapter
       -> TAR-family backend adapter
       -> security and resource-limit enforcement
       -> transactional extraction/packing
```

The backend seam is real because ZIP and TAR-family archives have materially different adapters. It is internal to the library. Backend crate types must not leak through the public interface.

## Public library interface

The initial `Archive` module owns these operations:

- open an `ArchiveLocator` and detect its format from content;
- enumerate normalized entry metadata;
- look up one entry;
- inspect archive-level information and capabilities;
- copy one entry to a caller-provided `Write` stream;
- verify all readable entry data;
- safely extract through an explicit extraction plan.

The interface intentionally prefers `copy_entry_to` over returning a borrowed decoder. ZIP entry readers often borrow archive state, while TAR readers are sequential. A caller-provided writer gives both adapters one streaming interface without self-referential types, whole-entry buffering, or backend leakage.

## Locator model

`ArchiveLocator` is an explicit source abstraction. v0.1 implements filesystem paths only. The type is kept separate from backend selection so a future nested-entry or remote source can supply bytes without changing command semantics.

Nested traversal will not use ambiguous path concatenation such as `outer.zip/inner.tar/file`. Its locator grammar and CLI syntax require an RFC. Candidate syntax includes an ordered `--within` chain, but no syntax is committed in v0.1.

## Format detection

Detection is content-first:

1. Read the required prefix/header bytes.
2. Match ZIP or Gzip signatures and validate the selected decoder.
3. Validate TAR header structure/checksum rather than relying only on `.tar`.
4. Use extensions only for output format selection during `pack`, or as diagnostic context.

A misleading input extension does not override valid magic bytes. A Gzip stream that is not a TAR archive is unsupported in v0.1 and must not be mislabeled TAR.GZ.

## Normalized entry model

An entry has a normalized archive path plus metadata that can be obtained without content-wide analysis: path encoding, kind, uncompressed size, optional compressed size, optional modified time, encryption indication when available, executable indication, optional link target, and optional integrity checksum.

Non-UTF-8 entry bytes are exposed with deterministic `%XX` escaping and `path_encoding: escaped_bytes`. Query commands can address the displayed value. v0.1 extraction refuses these names rather than silently creating a different filesystem path.

The model does not perform MIME sniffing by default and does not require a content hash. Unsupported or unavailable metadata is represented explicitly with nullable fields, not fabricated values.

Duplicate archive paths remain distinct during enumeration. Operations that name a single path reject ambiguity rather than silently selecting an entry.

## Capability model

Capabilities describe semantics and likely access cost:

- `random_access`: selected entries can be reached without sequentially decoding earlier content;
- `streaming_read`: entry bytes can be copied to a writer without whole-entry buffering;
- `encrypted`: the backend can read encrypted entries;
- `solid`: the format/archive uses solid compression;
- `can_create`, `can_extract`, `can_verify`, `can_seek`.

v0.1 ZIP reports random access and seek capability. TAR and TAR.GZ report sequential access. These flags describe the implemented adapter, not only theoretical format capability.

## Streaming model

Content operations use bounded buffers and `Read`/`Write` streaming. They must not call `read_to_end` for untrusted large entries. TAR.GZ may scan from the beginning to find a selected entry; `inspect` exposes this limitation through capabilities and warnings.

Enumeration may return one metadata vector because tree rendering, stat ambiguity checks, destination planning, and JSON output need a stable snapshot. A command must not create multiple redundant copies of that snapshot.

## Extraction model

Full extraction is plan-driven:

1. enumerate and validate every entry path and declared size;
2. reject links, special files, duplicate/case-insensitive collisions, non-UTF-8 names, unsafe paths, and resource-limit violations;
3. choose the explicit or intelligent destination;
4. extract into a temporary sibling staging location;
5. close and flush all files;
6. commit the completed staging tree to the destination;
7. leave the source untouched.

v0.1 refuses an existing destination. Later overwrite/skip/rename policies must preserve the plan-and-commit model.

Single-entry extraction writes a temporary sibling file and commits it only after the stream completes. Directory entries are not accepted as single-file output targets.

Cross-filesystem rename fallback is deliberately not needed when staging is created beside the destination. Future remote or user-selected staging locations must define a verified copy fallback before use.

## Intelligent destination rule

For `arcthis extract archive.ext`:

- if all entries are under one real top-level directory, that directory becomes the final destination and the common prefix is stripped during staging;
- otherwise, the final destination is a directory named from the full archive stem (`backup.tar.gz` -> `backup`), and entry paths remain unchanged;
- an explicit `--output` is always the extraction root and does not trigger prefix stripping;
- destination collisions fail before content is written.

## Transactional packing

Packing determines the output format from the requested output suffix, rejects source links/special files, writes to a temporary sibling file, finalizes and syncs the encoder, reopens and verifies the result through the normal `Archive` interface, and only then commits the output path without clobbering. Existing outputs are refused in v0.1.

## Error model

The library returns typed errors with stable public categories. Backend errors are translated at the seam. The CLI maps categories to structured machine errors and process exit codes. Internal crate names, debug representations, and backtraces are not a public interface.

## Future evolution

- Additional backend adapters can join the internal seam without changing command implementations.
- Indexes and seek points can be cached behind `Archive` when large TAR access justifies it.
- Nested locators can provide a stream obtained from an outer archive entry.
- MCP, HTTP, FFI, and language bindings can reuse the library interface.
- Async I/O is added only if a server/remote-source performance model demonstrates value.
