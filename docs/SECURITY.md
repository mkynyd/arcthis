# Security Model

## Trust boundary

Archive bytes, entry names, metadata, checksums, link targets, declared sizes, and compression ratios are untrusted. Filesystem paths and output destinations can also be adversarial or race-prone. Human-readable inspection warnings are supplementary; enforcement occurs on the content-writing path.

## Path safety

Before extraction, every entry path is normalized as an archive path and checked component by component. v0.1 rejects:

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

v0.1 rejects symlinks, hardlinks, block/character devices, FIFOs, sockets, and unknown entry kinds during extraction. This prevents a link created by one entry from redirecting a later write outside the root. Future link restoration requires an explicit option, target validation, order-independent planning, and regression tests.

## Resource limits

Extraction uses enforced limits with conservative defaults:

- maximum entry count (100,000 by default);
- maximum total declared and actual output bytes (16 GiB by default);
- maximum single-entry declared and actual bytes (4 GiB by default);
- maximum relative path length and component count.

Declared metadata is checked during planning. Actual bytes are counted while streaming because metadata can be missing or false. A limit violation aborts staging and leaves no committed destination.

v0.1 does not recurse into nested archives, which bounds nested bomb depth at zero.

## Staging and commit

Full extraction creates a temporary sibling directory on the destination filesystem. Content is written only inside it. After every entry succeeds and streams are closed, the staging directory is renamed to the absent final destination. Failure removes staging on best effort and preserves the source archive.

Single-entry extraction and packing use temporary sibling files and no-clobber commits only after streaming/finalization succeeds. Packing additionally syncs, reopens, and verifies the temporary archive before commit.

v0.1 refuses all destination collisions and does not implement overwrite. This avoids partial replacement and rollback ambiguity.

## Intelligent destination safety

Automatic destination selection occurs only after complete metadata enumeration and path validation. Compound suffixes are removed as a unit (`backup.tar.gz` -> `backup`). A common top-level prefix is used only when it is a real directory-like root, not merely a shared filename prefix.

## Verification

ZIP verification streams every file entry so decoder CRC validation runs. TAR and TAR.GZ verification parses every header and streams every file payload; Gzip checks are validated by reading the stream to completion. Verification does not claim cryptographic authenticity.

## Encryption and passwords

Encrypted archives are outside v0.1. The project must not accept passwords via ordinary command-line arguments in a future release because process listings and shell history can expose them. A future password interface should prefer prompt, file descriptor, or secret-provider input and must distinguish `password_required` from `wrong_password`.

## Delete-source lifecycle

`--delete-source` is planned, not implemented. Its required sequence is:

```text
perform -> close/finalize -> verify -> commit destination -> delete source
```

Any error, interruption, partial failure, verification failure, or commit failure must preserve the source. Deletion must target only the resolved source from the execution plan.

## Known limits of v0.1

- No defense against CPU exhaustion beyond byte/entry limits and backend behavior.
- No sandbox around codec crates.
- Metadata enumeration of a streaming TAR.GZ requires sequential decompression.
- v0.1 rejects non-UTF-8 entry names instead of preserving raw filesystem bytes, and does not perform full Unicode normalization collision analysis.
- Filesystem races by other processes cannot be eliminated completely; staging and collision refusal reduce exposure.
- Verification checks structural/codec integrity, not provenance or malicious content.

Security issues should be reported privately once the repository publishes a security contact. Do not include sensitive archive samples in public issues.
