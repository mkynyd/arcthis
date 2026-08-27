# Security Model

## Trust boundary

Archive bytes, entry names, metadata, checksums, link targets, declared sizes, and compression ratios are untrusted. Filesystem paths and output destinations can also be adversarial or race-prone. Human-readable inspection warnings are supplementary; enforcement occurs on the content-writing path.

## Path safety

Before extraction, every entry path is normalized as an archive path and checked component by component. The current implementation rejects:

- empty file paths;
- NUL bytes;
- absolute POSIX paths;
- `.` and `..` components;
- Windows drive prefixes, rooted paths, UNC/device prefixes, and backslash traversal;
- paths that normalize outside the extraction root;
- duplicate, case-insensitive, or parent/child conflicts that would make extraction filesystem- or order-dependent;
- non-UTF-8 entry names that cannot be materialized without changing the original bytes;
- paths whose platform representation cannot be handled safely.

The sanitized relative path is joined to a private staging root. Code must never pass an untrusted entry path directly to `Path::join` and then write.

## Links and special files

The current implementation rejects symlinks, hardlinks, block/character devices, FIFOs, sockets, and unknown entry kinds during extraction. This prevents a link created by one entry from redirecting a later write outside the root. Future link restoration requires an explicit option, target validation, order-independent planning, and regression tests.

## Resource limits

Extraction uses enforced limits with conservative defaults:

- maximum entry count (100,000 by default);
- maximum total declared and actual output bytes (16 GiB by default);
- maximum single-entry declared and actual bytes (4 GiB by default);
- maximum relative path length and component count.
- optional declared compression ratio per entry;
- optional wall-clock duration per streamed entry.

Declared metadata is checked during planning. Actual bytes are counted while streaming because metadata can be missing or false. A limit violation aborts staging and leaves no committed destination.

`extract-all --recursive` recurses through filesystem directories, not into archive contents.

Nested query traversal is separately bounded to depth 8 and 256 MiB decoded bytes per inner archive by default. `--max-nested-entry-size` can lower or raise the per-level bound. Allocation uses fallible reservation and actual streamed bytes are checked. Nested extraction and source deletion are not supported.

`grep` defaults to 16 MiB per entry, 10,000 matching lines, an 8 KiB binary probe, and at most 1 MiB retained per line. These are content-discovery bounds, not extraction guarantees; extraction enforces its independent limits.

## Staging and commit

Full extraction creates a temporary sibling directory on the destination filesystem. Content is written only inside it. After every entry succeeds and streams are closed, the staging directory is renamed to the absent final destination. Failure removes staging on best effort and preserves the source archive.

Single-entry extraction and packing use temporary sibling files and commit only after streaming/finalization succeeds. Packing additionally syncs, reopens, and verifies the temporary archive before commit.

Existing paths are canonicalized for lifecycle comparison, and missing destinations are resolved through their nearest existing ancestor. Source and destination aliases are rejected. A pack destination must be outside a directory source. When source deletion is requested, neither source nor destination may contain the other, so post-commit deletion cannot remove the result. These invariants are recorded in [ADR 0001](./ADR-0001-TRANSACTIONAL-LIFECYCLE.md).

Destination collisions are refused by default. `--skip-existing` performs no write and never deletes the source. `--rename` selects the first available numbered sibling. `--overwrite` first moves the old destination to a unique sibling backup, commits the staged replacement, restores the backup if commit fails, and removes the backup after success. Concurrent external filesystem mutation remains a race boundary.

## Intelligent destination safety

Automatic destination selection occurs only after complete metadata enumeration and path validation. Compound suffixes are removed as a unit (`backup.tar.gz` -> `backup`). A common top-level prefix is used only when it is a real directory-like root, not merely a shared filename prefix.

## Verification

ZIP, 7z, and RAR verification stream every readable entry so backend integrity checks run. TAR-family verification parses every header and streams every payload. Gzip, Bzip2, XZ, and Zstandard single streams are read to completion. Verification does not claim cryptographic authenticity.

## Encryption and passwords

Encrypted ZIP, 7z, and RAR archives are read through `--password-file`, which reads secret bytes from a file and strips trailing CR/LF. Passwords are never accepted as ordinary command-line arguments, keeping secrets out of process listings and shell history. Missing credentials map to `password_required`; incorrect credentials map to `wrong_password`. Passwords are redacted from library `Debug` output.

Encrypted archive **creation** is not implemented: `pack` rejects `--password-file` rather than silently ignoring it. RAR decryption support depends on libarchive and the RAR variant; unsupported encryption returns an explicit backend error instead of producing unreadable output.

## Delete-source lifecycle

`--delete-source` implements this sequence:

```text
perform -> close/finalize -> verify -> commit destination -> delete source
```

Any error, interruption, partial failure, verification failure, or commit failure must preserve the source. Deletion must target only the resolved source from the execution plan.

Selected-entry extraction verifies the complete source archive before commit when `--delete-source` is active. This deliberately trades additional decoding for confidence that an unselected corrupt entry is not discarded with the source.

Dry-run computes and serializes the destination, collision action, estimated sizes, warnings, and deletion intent but performs no writes, renames, or deletions. A skipped operation never deletes its source.

## Known limits of v0.4

- Duration enforcement is checked during output writes; a codec that blocks inside one decoder call cannot be preempted safely.
- No sandbox around codec crates.
- Metadata enumeration of compressed TAR and single-stream formats requires sequential decompression.
- Nested traversal currently buffers one selected inner archive in memory so its backend can reopen it; it does not yet provide range-backed nested seeking.
- Literal `grep` still decodes complete selected entries even after binary classification or match truncation when a backend integrity stream must be consumed.
- The current implementation rejects non-UTF-8 entry names instead of preserving raw filesystem bytes, and does not perform full Unicode normalization collision analysis.
- Multipart `--volume` sources cannot participate in `--delete-source`; deleting several volume files atomically is not yet specified.
- RAR metadata (solid, encryption, compressed-size) is limited by the libarchive adapter, so `inspect` may under-report; read/extract/verify remain authoritative. Format-native RAR multi-volume sets are not supported.
- Filesystem races by other processes cannot be eliminated completely; staging and collision refusal reduce exposure.
- Verification checks structural/codec integrity, not provenance or malicious content.

Security issues should be reported privately once the repository publishes a security contact. Do not include sensitive archive samples in public issues.
