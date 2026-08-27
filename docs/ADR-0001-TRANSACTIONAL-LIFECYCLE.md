# ADR 0001: Transactional lifecycle and source/destination separation

Status: accepted

## Context

`pack`, `extract`, and `convert` can replace destinations and optionally delete their sources. A successful codec call is not sufficient evidence that the operation is safe: a destination may alias the source, sit inside a source directory, or be removed when an ancestor source is deleted. Selected-entry extraction can also succeed while an unselected archive entry is corrupt.

These cases are especially dangerous for agent callers because an exit status of zero and `source_deleted: true` would falsely report a durable result.

## Decision

All destructive archive workflows use the shared lifecycle layer and follow:

```text
plan -> stage -> finalize -> verify -> commit destination -> delete source
```

The following invariants are enforced before writing:

- source and destination must not resolve to the same filesystem path;
- a pack destination must be outside a directory source, even without `--delete-source`, so an archive cannot include or replace its own output;
- when `--delete-source` is requested, neither path may contain the other;
- `--skip-existing` never deletes the source;
- single-entry extraction verifies the complete source archive before commit when source deletion is requested;
- failures before commit leave no final destination, and failures before source deletion preserve the source.

Existing paths are compared after canonicalization. Missing destinations are resolved through their nearest existing ancestor so symlinked parent directories do not bypass overlap checks.

## Consequences

- Some previously accepted but unsafe path combinations now fail with the stable `collision` error category.
- `extract <archive> <entry> --delete-source` can decode more than the selected entry because complete verification is intentionally required before irreversible deletion.
- Packing into a source directory is rejected. Callers must choose a sibling or external destination.
- Filesystem mutation by another process remains a race boundary; staging, canonical comparison, and collision handling reduce but cannot eliminate it.

## Verification

Regression tests must cover same-path aliases, destinations inside directory sources, preservation of source and destination on rejection, and corruption in an unselected entry before selected-entry source deletion.
