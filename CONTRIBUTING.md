# Contributing to arcthis

Thank you for helping build a reliable archive access layer for agents and humans.

## Before changing code

1. Read `README.md`, `START.md`, `docs/ARCHITECTURE.md`, `docs/SECURITY.md`, and `AGENTS.md`.
2. Check `ROADMAP.md` so planned behavior is not presented as implemented.
3. Keep format-specific behavior behind the archive backend seam.
4. Open an RFC or ADR before changing public CLI grammar, JSON field semantics, extraction safety, or the nested locator model.

## Development checks

Use Rust 1.98.0 through the checked-in toolchain file, then run:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release --locked
```

Changes to archive behavior need real dynamically built fixtures and CLI integration tests. Security fixes need regression tests that demonstrate both rejection and absence of committed output.

## Documentation

Update every affected English and Chinese user document. Keep `INDEX.md` aligned with important file responsibilities. Planned commands belong in `ROADMAP.md`, not in the current command list. `log.md` is a local Agent work log and is intentionally ignored by Git.

## Pull requests

Keep pull requests focused. Explain observable behavior, safety implications, JSON/CLI compatibility, and the commands used for verification. Do not include generated `target/` output, local indexes, credentials, or archive samples containing sensitive data.
