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

## Non-goals for v0.1

- Reimplementing Deflate, Gzip, ZIP, or TAR codecs.
- Replacing 7-Zip or `ouch` as a maximal-format archiver.
- FUSE mounting, TUI, preview/rendering, remote archives, server mode, or MCP frontend.
- Persistent indexing, nested traversal syntax, encrypted archives, multipart archives, RAR, 7z, XZ, or Zstandard.
- Content search commands (`find`, `grep`, `hash`) before the access and safety interfaces are stable.
- `--delete-source`, overwrite policies, archive conversion, or recursive batch extraction before a complete dry-run and transaction model exists.

## Influences

- `ouch`: unified multi-format operations and approachable CLI output.
- `atool`: stdout access, batch workflows, intelligent extraction destinations, and simulation.
- 7-Zip: verification, stdout streaming, overwrite policy, and delete-after-success lifecycles.
- `lsar`/`unar`: structured archive inspection and nested archive discovery.
- `ratarmount`: archive-as-filesystem, indexing, seek points, random access, and nested archive design.

These are validated design inputs, not command-by-command templates.

## Success criteria

v0.1 is successful when an agent can inspect an unknown supported archive, discover entries, stream one entry without materializing the full archive, safely extract selected or complete contents, create an archive transactionally, verify integrity, and determine every outcome from stdout, stderr, JSON, and process exit status.
