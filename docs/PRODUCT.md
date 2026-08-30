# Product Definition

## Positioning

**An agent-native command-line tool for accessing and changing compressed files.**

`arcthis` is one unified tool for AI agents and humans. It lets callers inspect, list, read, verify, and selectively extract archive contents through filesystem-like operations that work the same way across formats.

The central idea is **the archive as a browsable filesystem**. A caller should first learn what an archive contains, then decode or extract only what it actually needs.

## Problem

Agents commonly handle an archive by extracting everything into a temporary directory, recursively scanning it, reading a few relevant files, and deleting the rest. For source bundles, datasets, backups, logs, and media collections, this workflow causes avoidable reads and writes, storage use, delays, and cleanup risk.

`arcthis` changes the default workflow:

```text
inspect -> tree/list -> stat/read -> extract only when needed
```

"Not unpacking the whole archive" does not mean "without decoding." ZIP can usually decode one selected file. TAR.GZ must often be decoded from start to finish. A solid 7z archive may require decoding a larger compression block. `arcthis` exposes these differences as capabilities and warnings rather than pretending every format has the same cost.

## Product principles

1. **Look before you extract.** Cheap discovery comes before writing to disk.
2. **Read before you write.** File bytes go to stdout or a caller-provided writer when possible.
3. **Structured by default for agents.** Query commands expose stable, structured JSON.
4. **Safe before convenient.** Extraction and destructive processes are conservative, validated, and all-or-nothing.
5. **One behavior across formats.** Each format's implementation owns its differences; commands use one archive model.
6. **Compose instead of reimplement.** `read` composes with `rg`, `jq`, `ffprobe`, `pandoc`, and other Unix tools.
7. **Library first, CLI first-class.** Archive access is reusable while command-line usability remains a product requirement.
8. **Do not extract what you do not need.** Full extraction is an explicit operation, not an implementation shortcut.

## Users and workflows

### AI agents

Agents need deterministic exit status, strict stdout/stderr separation, stable JSON, capped resource use, and enough capability metadata to estimate the cost of an operation.

### Humans

Humans need concise terminal output, familiar Unix pipelines, predictable destinations, helpful errors, and safe defaults. TTY decoration is optional presentation, never part of the data format.

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
- Writing to disk: safe `extract`, all-or-nothing `pack`.
- Integrity: `verify`.
- Interfaces: human output and versioned JSON for structured query commands.
- Safety: path traversal rejection, conservative link/special-file handling, collision refusal, configurable capped extraction, and write-then-save.
- Platforms: macOS and Linux; Windows-portable design without a v0.1 support guarantee.

## v0.2 scope

v0.2 extends the trusted tool without changing the access-first workflow:

- Formats: 7z, TAR.BZ2/TBZ2, TAR.XZ/TXZ, TAR.ZST/TZST, plus GZIP, BZIP2, XZ, and Zstandard single-file formats.
- Creation: all implemented container and single-file formats can be produced through `pack`, finished, reopened, and verified before saving.
- Batch extraction: `extract-all` performs capped parallel work across independent archives and supports recursive discovery.
- Planning: `extract`, `extract-all`, and `pack` expose structured `--dry-run` plans.
- Lifecycle: `--delete-source` runs only after successful verification and destination save; skipped and failed operations retain the source.
- Collisions: refusal remains the default, with explicit mutually exclusive overwrite, skip, and rename choices.
- Limits: optional declared compression-ratio and per-file duration enforcement supplements size and file-count limits.

## v0.3 scope

v0.3 adds agent-native discovery while preserving capped read behavior:

- Path discovery: `find` applies glob filters to normalized file metadata without content decoding.
- Content discovery: literal `grep` reads regular files with binary, file-size, line-size, and match-count limits.
- Integrity commands: `hash` checksums one file through SHA-256 or SHA-512.
- Metadata: files expose stable archive order and lightweight extension-based MIME guesses through a reusable in-process metadata index.
- Nested access: repeatable `--within` chains open selected inner archive files through capped read-only memory buffers without named temporary files.

## v0.4 scope

v0.4 expands compatibility without weakening the access and lifecycle rules:

- RAR/RAR5: content-first read, query, extract, and verify through a statically linked libarchive integration; creation remains unsupported and licensing/redistribution boundaries are documented.
- Encryption: file-based password input for encrypted ZIP and 7z access, with stable missing/wrong-password errors and no secret CLI argument.
- Split sources: explicit ordered split files exposed through repeatable `--volume`; native RAR volume protocols remain outside this source model.
- Persistent metadata caches: single-step create/refresh/delete, dry-run actions, source fingerprint invalidation, and a configurable cache root.
- Conversion: source safety check, capped temporary extraction, verified target packing, shared collision handling, and post-save `--delete-source`.

## v0.5 scope

v0.5 makes the same archive behavior directly usable by local agent runtimes:

- Application service: structured interface-independent inspect/list/tree/stat/read/find/grep/hash/verify requests with finite limits and cancellation checkpoints.
- Local MCP: built-in stdio transport with declared formats, structured results, input-root authorization, capped text/base64 windows, and strict stdout separation.
- Controlled change: opt-in output roots and plan/execute tools for extract, pack, and convert; stale source/destination state invalidates a SHA-256 plan digest before any change.
- Destructive policy: source deletion remains disabled by default, requires two explicit opt-ins, and still runs only after a verified save.
- Compatibility: public installation channels include MCP by default so they expose the same commands; library-only builds may use `--no-default-features`. The server is covered by subprocess clients, official Inspector validation, and macOS/Linux automated tests.

## Current non-goals

- Reimplementing Deflate, Gzip, ZIP, or TAR codecs.
- Replacing 7-Zip or `ouch` as a maximal-format archiver.
- FUSE mounting, TUI, preview/rendering, remote archives, or network server mode.
- Encrypted archive creation, password values in process arguments, or a full secret-provider system.
- Format-native RAR multipart traversal, RAR creation, or claims beyond libarchive's documented closed-format limitations.
- Cross-archive recursive search.
- Seek-point/content indexes beyond the implemented metadata cache.

## Influences

- `ouch`: unified multi-format operations and approachable CLI output.
- `atool`: stdout access, batch workflows, intelligent extraction destinations, and simulation.
- 7-Zip: verification, stdout output, overwrite policy, and delete-after-success lifecycles.
- `lsar`/`unar`: structured archive inspection and nested archive discovery.
- `ratarmount`: archive-as-filesystem, indexing, seek points, random access, and nested archive design.

These are validated design inputs, not command-by-command templates.

## Success criteria

v0.1 is successful when an agent can inspect an unknown supported archive, discover files, read one file without writing the full archive to disk, safely extract selected or complete contents, create an archive all-or-nothing, verify integrity, and determine every outcome from stdout, stderr, JSON, and process exit status.

v0.3 is successful when the same agent can cheaply filter paths, perform capped text search and hashing, and explicitly traverse supported inner archives while receiving stable structured costs and limit failures.

v0.4 is successful when the agent can use the same access/query commands on tested RAR input, explicitly supply encrypted ZIP/7z credentials and split files, manage a persistent metadata cache, and execute a dry-runnable conversion whose target is verified before saving or source deletion.

v0.5 is successful when a local MCP client can discover capped structured tools, read every supported archive family without shell parsing, and perform explicitly authorized plan/execute changes while stale plans, path escapes, cancellation, and unverified deletion preserve existing state.
