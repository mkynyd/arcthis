# Third-party licenses

This file summarizes the licenses of the dependencies that `arcthis` links or bundles. It is a maintenance aid for redistribution review, not legal advice. The exact license texts shipped with each crate in the Cargo registry and with each native library are authoritative; `cargo tree` and the local `Cargo.lock` describe the resolved set for a given build.

`arcthis` itself is MIT ([LICENSE](./LICENSE)).

## Rust dependencies

Direct dependencies and their declared licenses:

| Crate | Version | License | Purpose |
| --- | --- | --- | --- |
| `bzip2` | 0.6 | MIT OR Apache-2.0 | Bzip2 single-stream codec (delegates to pure-Rust `libbz2-rs-sys`) |
| `base64` | 0.23 | MIT OR Apache-2.0 | Optional MCP binary-window encoding |
| `clap` | 4.6 | MIT OR Apache-2.0 | CLI grammar and help |
| `compress-tools` | 0.16 | MIT OR Apache-2.0 | libarchive adapter used by the RAR backend |
| `directories` | 6.0 | MIT OR Apache-2.0 | Platform cache directory for persistent indexes |
| `flate2` | 1.1 | MIT OR Apache-2.0 | Gzip/Deflate codec |
| `globset` | 0.4 | Unlicense OR MIT | Glob matching for `find`/`grep` |
| `lzma-rust2` | 0.20 | Apache-2.0 | Pure-Rust XZ/LZMA codec |
| `mime_guess` | 2.0 | MIT | Lightweight MIME guess on entry names |
| `rmcp` | 3.1 | Apache-2.0 | Optional MCP `2025-06-18` server and stdio transport |
| `schemars` | 1.2 | MIT | Optional MCP input/output JSON Schema generation |
| `serde` / `serde_json` | 1.0 | MIT OR Apache-2.0 | Stable JSON schema serialization |
| `sha2` | 0.11 | MIT OR Apache-2.0 | SHA-256/SHA-512 streaming digests |
| `sevenz-rust2` | 0.22 | Apache-2.0 | Pure-Rust 7z read/write (AES-256, bzip2, deflate, ppmd, zstd) |
| `tar` | 0.4 | MIT OR Apache-2.0 | TAR container codec |
| `tempfile` | 3.27 | MIT OR Apache-2.0 | Staging directories and sibling files |
| `thiserror` | 2.0 | MIT OR Apache-2.0 | Typed library errors |
| `time` | 0.3 | MIT OR Apache-2.0 | Timestamp formatting |
| `tokio` | 1.53 | MIT | Optional MCP stdio task runtime and cancellation bridge |
| `walkdir` | 2.5 | Unlicense OR MIT | Filesystem recursion for `extract-all` |
| `zip` | 8.2 | MIT | ZIP read/write and AES decryption |
| `zstd` | 0.13 | MIT | Zstandard codec (links `zstd-sys`) |

Dev dependency: `assert_cmd` (MIT OR Apache-2.0) for CLI integration tests.

## Native libraries

Release builds embed native code through the RAR backend and the Zstandard codec. The final executable is not necessarily fully static and can dynamically link platform libraries; inspect each release artifact (`otool -L` on macOS or `ldd`/`readelf` on Linux) before redistribution.

| Library | License | Route |
| --- | --- | --- |
| libarchive (3.2.0+) | BSD-style: 2-clause BSD for most sources, plus a few 3-clause UC Regents files | `compress-tools` `static` build feature; the final binary can still use dynamic platform codec/XML libraries |
| zstd | BSD-3-Clause (also dual-licensed GPLv2) | `zstd-sys` |
| libbz2 (Rust port) | `bzip2-1.0.6` license | `libbz2-rs-sys`, a pure-Rust reimplementation used by the `bzip2` crate |

Other codecs — Gzip/Deflate, XZ/LZMA, 7z, TAR, and Bzip2 as used directly — are pure-Rust and do not add native linking beyond the table above.

## Redistribution notes

- libarchive's BSD-style license requires retaining its copyright notice when distributing statically linked binaries. The RAR backend is the sole consumer; see [docs/RAR.md](./docs/RAR.md).
- The RAR format is proprietary to RARLAB. `arcthis` reads RAR through libarchive and never writes RAR, so no UnRAR or RARLAB SDK redistribution obligation is introduced.
- Release binaries may embed zstd's BSD-3-Clause and libarchive code while dynamically using additional platform libraries; keep this file and per-platform artifact audits current if the linked set changes.

Build prerequisites for the native path are documented in [README.md](./README.md) and [START.md](./START.md).
