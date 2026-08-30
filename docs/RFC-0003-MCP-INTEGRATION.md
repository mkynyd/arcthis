# RFC-0003: MCP integration contract

Status: accepted and implemented in v0.5

## Decision

`arcthis` will expose MCP through a feature-gated `arcthis mcp` stdio frontend over the frontend-neutral application service. The core archive and application modules remain synchronous and have no dependency on MCP, Clap, terminal formatting, or an async runtime.

The pinned protocol revision is `2025-06-18`. A server may negotiate another revision only when covered by protocol tests. JSON-RPC messages are the only bytes written to stdout; diagnostics and logs go to stderr.

## Tool names and operations

Read-only tools use the `archive_` prefix: `archive_inspect`, `archive_list`, `archive_tree`, `archive_stat`, `archive_find`, `archive_grep`, `archive_hash`, and `archive_verify`. Entry content is read with `archive_read`, which always requires an explicit byte `offset` and bounded `length` and returns text or base64 data plus `eof` and the raw byte count.

Mutation tools are `archive_extract_plan`, `archive_extract_execute`, `archive_pack_plan`, `archive_pack_execute`, `archive_convert_plan`, and `archive_convert_execute`. They are unavailable unless the server has an explicit output-root policy. Execution requires the exact digest returned by planning.

Every tool declares JSON input and output schemas. Successful structured output is also mirrored as JSON text content for clients that do not consume `structuredContent`. Errors use the existing stable arcthis error codes and do not expose Rust backtraces.

## Reserved resource URI grammar

v0.5 exposes bounded entry content through `archive_read`; it does not advertise MCP resources. If a later revision adds resources, it must use the following grammar rather than reinterpret filesystem or diagnostic paths:

Entry resources use:

`arcthis://archive/<source-id>/entry/<percent-encoded-entry-path>?offset=<u64>&length=<u64>`

`source-id` is an opaque server-issued identifier, not a filesystem path. Entry paths are percent encoded UTF-8 archive paths. Ambiguous `::` path grammar is forbidden. Nested archives are selected through an explicit repeated `within` input and receive a distinct source identifier.

Resources never imply unlimited reads. `offset` and `length` are mandatory and subject to the same decoded-byte and read-window limits as `archive_read`. Binary content uses MCP blob content with standard base64; text is returned only when the selected bytes are valid UTF-8 and contain no NUL byte.

## Local path policy

The server starts with one or more canonical allowed input roots. Empty input roots mean no filesystem source is authorized. Paths are canonicalized before use and must be equal to or descendants of an allowed root. Symlink traversal cannot escape an allowed root. MCP client roots are discovery hints only and never grant access.

Mutation additionally requires canonical allowed output roots. Existing arcthis path sanitation, collision policy, staging, verification, source/destination separation, and resource limits remain mandatory. Password values are never accepted as ordinary MCP inputs; a later secret-provider policy may supply protected password references.

## Limits and cancellation

The server has finite defaults for archive entries, decoded bytes, result count, read-window bytes, nested depth, and nested entry bytes. Request overrides may only lower server limits. Cancellation is cooperative: the application service checks before opening, after metadata enumeration, between query phases, and during streamed writer calls. Mutation execution checks before planning and after immediate pre-perform revalidation; the synchronous transactional core then preserves its existing cleanup and source-preservation guarantees but cannot preempt a codec call already in progress.

## Mutation plan binding

A mutation plan digest is SHA-256 over a canonical versioned representation containing operation, canonical source fingerprint, canonical destination, limits, collision policy, delete-source policy, and operation-specific inputs. Execution re-plans immediately before writing and rejects a stale digest. The source fingerprint includes canonical path, file type, byte length, modified timestamp, and content hash for files; directory packing fingerprints the deterministic input manifest and metadata.

No mutation tool enables source deletion unless both server policy and the request explicitly allow it. Destructive execution is separately named and annotated.

## Protocol and packaging verification

The feature-gated binary is tested as a subprocess for initialization, tool discovery, schema validity, malformed JSON-RPC, cancellation, bounded reads, path denial, stdout purity, and graceful EOF. Compatibility is exercised with the official MCP inspector/client and a second independent JSON-RPC client harness on macOS and Linux.
