# ADR 0002: explicit entry identity and JSON schema 2

Status: accepted for the unreleased checkout, 2026-10-15.

The complete exploration iteration is authorized. Schema 1 cannot express unknown executable state or distinguish an unknown ZIP local timestamp from an absolute timestamp consistently. CLI and MCP also serialize the same read-only operations differently.

The unreleased checkout moves the public result `schema_version` from `"1"` to `"2"`. Published 0.5.1 remains schema 1. Consumers must check the version and migrate before using this checkout. No release is performed by this work.

- Entries carry `path_raw_hex` as the adapter-exposed name bytes (ZIP/TAR/Gzip byte names; UTF-8 for Unicode names exposed by 7z/RAR libraries) and retain `archive_index` as instance identity. Name-based selection refuses ambiguity; explicit index selection bypasses only name ambiguity, never materialization safety. Extraction carries that identity through decoding and uses a common bounded writer.
- `executable` is nullable; directories and unknown modes are null. Directory CRC is null. Unknown or shared-block compressed sizes are null, not zero.
- `modified_time` is an absolute RFC3339 timestamp or null. ZIP DOS timestamps with unknown timezone move to `modified_time_local`; the implementation must not invent a UTC offset.
- CLI and MCP share application-service read-only envelopes including version and archive reference. Errors carry a stable code; MCP parameter errors are normalized rather than exposing serde diagnostics. JSON object key order has no semantic significance.
- Safe lexical normalization removes `.` and redundant interior separators, rejects every `..`, and detects collisions after normalization. A root directory marker is not a materialized child.
- ZIP duplicate directory instances use bounded single-entry directory views over the original seekable source. Existing ZIP codecs perform parsing, decryption, decoding and CRC. Compression algorithms are not reimplemented, and no archive is extracted or copied to serve a read.
- Raw `read` remains streaming by default and may emit bytes before failure. `--verify-first` stages a bounded entry in an anonymous temporary file and emits only after successful validation. Source deletion is never implied.
- Explicit name decoding, stdin spooling, durability reduction, batch continuation and per-request MCP password files retain finite limits and existing root/path/lifecycle checks. No unsafe-path skipping or link restoration is added.

Index documents use a separate updated internal version and invalidate old metadata. Release notes and bilingual guides describe migration and cost tradeoffs.
