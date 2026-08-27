# RAR support

Status: implemented for read/extract/verify in v0.4

## Scope

`arcthis` supports RAR and RAR5 archives for **read, extract, and verify only**. RAR **creation** is intentionally unsupported: `pack` and `convert` never invoke a RAR writer, and `--output` with a `.rar` suffix is rejected. Capabilities are reported per archive so callers can distinguish read, extract, create, and verify support instead of assuming a format works uniformly.

## Detection

Detection is content-first, not extension-based, in `src/archive/detect.rs`:

| Variant | Signature | Match |
| --- | --- | --- |
| RAR 4.x | `52 61 72 21 1A 07 00` (`Rar!\x1A\x07\x00`) | first 7 bytes |
| RAR 5.x | `52 61 72 21 1A 07 01 00` (`Rar!\x1A\x07\x01\x00`) | first 8 bytes |

Both map to the single `rar` archive format and the same backend.

## Backend

The RAR backend (`src/archive/backend/rar.rs`) is a thin adapter over **libarchive** through the `compress-tools` crate. `Cargo.toml` enables its `static` build feature, which builds and embeds the libarchive integration instead of requiring a separately installed dynamic libarchive at runtime. This does **not** make the complete executable fully static: the current macOS release still dynamically links platform libraries such as libxml2, zlib, bzip2, libiconv, ICU, and Expat. The exact linked set is platform- and toolchain-dependent and must be audited on each release target.

The adapter exposes the unified `ArchiveBackend` seam: `entries`, `copy_entry_to`, `extract_plan`, and `verify` all stream through libarchive's iterator. Because libarchive does not expose every RAR property, the backend reports:

- sequential access only (`random_access: false`, `can_seek: false`);
- `rar_metadata_limited` inspection warning — solid, encryption, and compressed-size metadata may be unavailable;
- `sequential_access` inspection warning — selected entry access may require a sequential scan.

`read`/`extract`/`verify` behavior is authoritative even when `inspect` cannot fully describe the archive.

## Encryption

Encrypted RAR is accepted through the same `--password-file` interface as ZIP and 7z. Actual support depends on libarchive and the RAR variant:

- RAR5 uses AES-256 (RAR5.0+) or AES-128 (initial RAR5.0) key derivation;
- RAR4 uses a proprietary scheme.

When libarchive cannot decrypt, `arcthis` returns a stable backend error rather than silently producing garbage. Missing credentials map to `password_required`; incorrect credentials map to `wrong_password`. Passwords are redacted from library `Debug` output and are never accepted as ordinary CLI values.

## Multipart and native volumes

The v0.4 `--volume` option concatenates arbitrary **byte-stream** segments before content detection (see [RFC 0002](./RFC-0002-MULTIPART-SOURCES.md)). It does **not** implement RAR's format-native multi-volume protocol (`.part1.rar`/`.part2.rar`/`.r00` sets). A RAR volume set is not a simple byte concatenation and cannot be reconstructed with `--volume`. Format-native RAR volume traversal remains a non-goal and is documented in the roadmap.

## Licensing and redistribution

`arcthis` does not implement RAR algorithms and does not link UnRAR or the RARLAB SDK. Decompression is delegated to libarchive's independently developed, BSD-licensed read-only implementation.

- **libarchive** is distributed under a BSD-style license (2-clause BSD for the bulk of the sources, with a few 3-clause UC Regents files). Redistributors must retain the applicable copyright notices for embedded native code; see [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md).
- **compress-tools** is dual-licensed MIT OR Apache-2.0.
- The **RAR format** itself is proprietary to RARLAB. Because `arcthis` only reads through libarchive and never writes RAR, it does not impose the redistribution obligations of a RAR encoder.

Anyone redistributing release binaries is responsible for auditing the final linked set; the summary above is guidance, not legal advice.

## Build prerequisites

RAR support requires libarchive and its development dependencies at build time:

- macOS (Homebrew): `libarchive libb2 bzip2 lz4 xz zstd`
- Debian/Ubuntu: `libarchive-dev libb2-dev libbz2-dev liblz4-dev liblzma-dev libxml2-dev libzstd-dev zlib1g-dev`

The other backends build without these packages.
