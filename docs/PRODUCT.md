# Product Definition

## Positioning

**An agent-native CLI for accessing and manipulating compressed files.**

`arcthis` is a unified archive access layer for AI agents and humans. It lets callers inspect, enumerate, stream, verify, and selectively materialize archive contents through format-independent filesystem-like operations.

The central idea is **archive as an accessible filesystem**. A caller should first learn what an archive contains and then decode or extract only what it actually needs.

## Problem

Agents commonly handle an archive by extracting everything into a temporary directory, recursively scanning it, reading a few relevant files, and deleting the rest. For source bundles, datasets, backups, logs, and media collections, this workflow causes avoidable I/O, storage use, latency, and cleanup risk.

`arcthis` changes the default workflow:

```text
inspect -> tree/list -> stat/read -> extract only when needed
```

"Without materializing the entire archive" does not mean "without decoding." ZIP can usually decode one selected entry. TAR.GZ must often be decoded sequentially. A solid 7z archive may require decoding a larger compression block. `arcthis` exposes these differences as capabilities and warnings rather than pretending every format has the same cost.

## Product principles

1. **Access before extraction.** Cheap discovery precedes disk materialization.
2. **Stream before materialize.** Entry bytes go to stdout or a caller-provided writer when possible.
3. **Structured by default for agents.** Query commands expose stable, typed JSON.
4. **Safe before convenient.** Extraction and destructive lifecycles are conservative, validated, and transactional.
5. **Unified semantics over format quirks.** Backends own format differences; commands use one archive model.
6. **Compose instead of reimplement.** `read` composes with `rg`, `jq`, `ffprobe`, `pandoc`, and other Unix tools.
7. **Library first, CLI first-class.** Archive access is reusable while CLI ergonomics remain a product requirement.
8. **Do not extract what you do not need.** Full extraction is an explicit operation, not an implementation shortcut.

## Users and workflows

### AI agents

Agents need deterministic exit status, strict stdout/stderr separation, stable JSON, bounded resource use, and enough capability metadata to estimate the cost of an operation.

### Humans

Humans need concise terminal output, familiar Unix pipelines, predictable destinations, helpful errors, and safe defaults. TTY decoration is optional presentation, never part of the data contract.

### Representative workflow

```sh
arcthis inspect dataset.zip --json
arcthis tree dataset.zip --json
arcthis stat dataset.zip train/data.csv --json
arcthis read dataset.zip train/data.csv | head
arcthis extract dataset.zip train/data.csv --output ./data.csv
```

## v0.1 scope

v0.1 establishes the trusted vertical path:

- Formats: ZIP (Stored/Deflate content in the current build), TAR, TAR.GZ.
- Access: `list`, `tree`, `stat`, `inspect`, `read`.
- Materialization: safe `extract`, transactional `pack`.
- Integrity: `verify`.
- Interfaces: human output and schema-versioned JSON for structured query commands.
- Safety: path traversal rejection, conservative link/special-file handling, collision refusal, configurable bounded extraction, and staging commits.
- Platforms: macOS and Linux; Windows-portable design without a v0.1 support guarantee.

## v0.2 scope

v0.2 extends the trusted access layer without changing the access-first workflow:

- Formats: 7z, TAR.BZ2/TBZ2, TAR.XZ/TXZ, TAR.ZST/TZST, plus GZIP, BZIP2, XZ, and Zstandard single-stream payloads.
- Creation: all implemented container and single-stream formats can be produced through `pack`, finalized, reopened, and verified before commit.
- Batch extraction: `extract-all` performs bounded parallel work across independent archives and supports recursive discovery.
- Planning: `extract`, `extract-all`, and `pack` expose typed `--dry-run` plans.
- Lifecycle: `--delete-source` runs only after successful verification and destination commit; skipped and failed operations retain the source.
- Collisions: refusal remains the default, with explicit mutually exclusive overwrite, skip, and rename policies.
- Limits: optional declared compression-ratio and per-entry streaming-duration enforcement supplements size and entry-count limits.

## v0.3 scope

v0.3 adds agent-native discovery while preserving bounded streaming behavior:

- Path discovery: `find` applies glob filters to normalized entry metadata without content decoding.
- Content discovery: literal `grep` streams regular files with binary, entry-size, line-size, and match-count bounds.
- Integrity primitives: `hash` streams one entry through SHA-256 or SHA-512.
- Metadata: entries expose stable archive order and lightweight extension-based MIME guesses through a reusable in-process metadata index.
- Nested access: repeatable `--within` chains open selected inner archive entries through bounded immutable memory sources without named temporary files.

## v0.4 scope

v0.4 expands compatibility without weakening the access and lifecycle contracts:

- RAR/RAR5: content-first read, query, extract, and verify through a statically linked libarchive adapter; creation remains unsupported and licensing/redistribution boundaries are documented.
- Encryption: file-based password input for encrypted ZIP and 7z access, with stable missing/wrong-password errors and no secret CLI argument.
- Multipart sources: explicit ordered byte-stream volumes exposed through repeatable `--volume`; native RAR volume protocols remain outside this source model.
- Persistent metadata indexes: transactional create/refresh/delete, dry-run actions, source fingerprint invalidation, and a configurable cache root.
- Conversion: source safety preflight, bounded temporary materialization, verified target packing, shared collision policy, and post-commit `--delete-source`.

## Current non-goals

- Reimplementing Deflate, Gzip, ZIP, or TAR codecs.
- Replacing 7-Zip or `ouch` as a maximal-format archiver.
- FUSE mounting, TUI, preview/rendering, remote archives, server mode, or MCP frontend.
- Encrypted archive creation, password values in process arguments, or a full secret-provider system.
- Format-native RAR multipart traversal, RAR creation, or claims beyond libarchive's documented proprietary-format limitations.
- Cross-archive recursive search.
- Seek-point/content indexes beyond the implemented metadata cache.

## Influences

- `ouch`: unified multi-format operations and approachable CLI output.
- `atool`: stdout access, batch workflows, intelligent extraction destinations, and simulation.
- 7-Zip: verification, stdout streaming, overwrite policy, and delete-after-success lifecycles.
- `lsar`/`unar`: structured archive inspection and nested archive discovery.
- `ratarmount`: archive-as-filesystem, indexing, seek points, random access, and nested archive design.

These are validated design inputs, not command-by-command templates.

## Success criteria

v0.1 is successful when an agent can inspect an unknown supported archive, discover entries, stream one entry without materializing the full archive, safely extract selected or complete contents, create an archive transactionally, verify integrity, and determine every outcome from stdout, stderr, JSON, and process exit status.

v0.3 is successful when the same agent can cheaply filter paths, perform bounded text search and hashing, and explicitly traverse supported inner archives while receiving stable structured costs and limit failures.

v0.4 is successful when the agent can use the same access/query primitives on tested RAR input, explicitly supply encrypted ZIP/7z credentials and byte-stream volumes, manage a persistent metadata cache, and execute a dry-runnable conversion whose target is verified before commit or source deletion.
