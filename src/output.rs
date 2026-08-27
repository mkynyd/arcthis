use std::io::{self, Write};
use std::path::Path;

use serde::Serialize;

use crate::SCHEMA_VERSION;
use crate::batch::{ExtractAllPlan, ExtractAllResult};
use crate::convert::{ConvertPlan, ConvertResult};
use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractPlan, ExtractResult};
use crate::index::IndexResult;
use crate::model::{ArchiveEntry, ArchiveFormat, ArchiveInspection, EntryKind, VerificationResult};
use crate::pack::{PackPlan, PackResult};
use crate::query::{FindResult, GrepResult, HashResult};

#[derive(Debug, Serialize)]
struct ArchiveReference {
    path: String,
    path_lossy: bool,
    format: ArchiveFormat,
}

impl ArchiveReference {
    fn new(path: &Path, format: ArchiveFormat) -> Self {
        ArchiveReference {
            path: path.to_string_lossy().into_owned(),
            path_lossy: path.to_str().is_none(),
            format,
        }
    }
}

#[derive(Debug, Serialize)]
struct ListResponse<'a> {
    schema_version: &'static str,
    archive: ArchiveReference,
    entries: &'a [ArchiveEntry],
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TreeNode {
    name: String,
    path: String,
    kind: EntryKind,
    entry: Option<ArchiveEntry>,
    children: Vec<Self>,
}

#[derive(Debug, Serialize)]
struct TreeResponse<'a> {
    schema_version: &'static str,
    archive: ArchiveReference,
    tree: &'a [TreeNode],
}

#[derive(Debug, Serialize)]
struct StatResponse<'a> {
    schema_version: &'static str,
    archive: ArchiveReference,
    entry: &'a ArchiveEntry,
}

#[derive(Debug, Serialize)]
struct InspectResponse<'a> {
    schema_version: &'static str,
    archive: ArchiveReference,
    #[serde(flatten)]
    inspection: &'a ArchiveInspection,
}

#[derive(Debug, Serialize)]
struct ExtractResponse<'a> {
    schema_version: &'static str,
    archive: ArchiveReference,
    extraction: &'a ExtractResult,
}

#[derive(Debug, Serialize)]
struct ExtractPlanResponse<'a> {
    schema_version: &'static str,
    operation: &'static str,
    plan: &'a ExtractPlan,
}

#[derive(Debug, Serialize)]
struct ExtractAllPlanResponse<'a> {
    schema_version: &'static str,
    operation: &'static str,
    plan: &'a ExtractAllPlan,
}

#[derive(Debug, Serialize)]
struct ExtractAllResponse<'a> {
    schema_version: &'static str,
    operation: &'static str,
    result: &'a ExtractAllResult,
}

#[derive(Debug, Serialize)]
struct VerifyResponse<'a> {
    schema_version: &'static str,
    archive: ArchiveReference,
    verification: &'a VerificationResult,
}

#[derive(Debug, Serialize)]
struct PackResponse<'a> {
    schema_version: &'static str,
    pack: &'a PackResult,
}

#[derive(Debug, Serialize)]
struct PackPlanResponse<'a> {
    schema_version: &'static str,
    operation: &'static str,
    plan: &'a PackPlan,
}

#[derive(Debug, Serialize)]
struct FindResponse<'a> {
    schema_version: &'static str,
    archive: ArchiveReference,
    find: &'a FindResult,
}

#[derive(Debug, Serialize)]
struct GrepResponse<'a> {
    schema_version: &'static str,
    archive: ArchiveReference,
    grep: &'a GrepResult,
}

#[derive(Debug, Serialize)]
struct HashResponse<'a> {
    schema_version: &'static str,
    archive: ArchiveReference,
    hash: &'a HashResult,
}

#[derive(Debug, Serialize)]
struct IndexResponse<'a> {
    schema_version: &'static str,
    operation: &'static str,
    index: &'a IndexResult,
}

#[derive(Debug, Serialize)]
struct ConvertResponse<'a> {
    schema_version: &'static str,
    convert: &'a ConvertResult,
}

#[derive(Debug, Serialize)]
struct ConvertPlanResponse<'a> {
    schema_version: &'static str,
    operation: &'static str,
    plan: &'a ConvertPlan,
}

pub(crate) fn write_find(
    writer: &mut impl Write,
    path: &Path,
    format: ArchiveFormat,
    result: &FindResult,
    json: bool,
) -> Result<()> {
    if json {
        return write_json(
            writer,
            &FindResponse {
                schema_version: SCHEMA_VERSION,
                archive: ArchiveReference::new(path, format),
                find: result,
            },
        );
    }
    for entry in &result.entries {
        writeln!(writer, "{}", entry.path)
            .map_err(|error| ArcthisError::io("writing find output", error))?;
    }
    Ok(())
}

pub(crate) fn write_grep(
    writer: &mut impl Write,
    path: &Path,
    format: ArchiveFormat,
    result: &GrepResult,
    json: bool,
) -> Result<()> {
    if json {
        return write_json(
            writer,
            &GrepResponse {
                schema_version: SCHEMA_VERSION,
                archive: ArchiveReference::new(path, format),
                grep: result,
            },
        );
    }
    for matched in &result.matches {
        writeln!(
            writer,
            "{}:{}:{}",
            matched.path, matched.line_number, matched.text
        )
        .map_err(|error| ArcthisError::io("writing grep output", error))?;
    }
    Ok(())
}

pub(crate) fn write_hash(
    writer: &mut impl Write,
    path: &Path,
    format: ArchiveFormat,
    result: &HashResult,
    json: bool,
) -> Result<()> {
    if json {
        return write_json(
            writer,
            &HashResponse {
                schema_version: SCHEMA_VERSION,
                archive: ArchiveReference::new(path, format),
                hash: result,
            },
        );
    }
    writeln!(writer, "{}  {}", result.digest, result.entry)
        .map_err(|error| ArcthisError::io("writing hash output", error))
}

pub(crate) fn write_list(
    writer: &mut impl Write,
    path: &Path,
    format: ArchiveFormat,
    entries: &[ArchiveEntry],
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &ListResponse {
                schema_version: SCHEMA_VERSION,
                archive: ArchiveReference::new(path, format),
                entries,
            },
        )
    } else {
        writeln!(writer, "KIND\tSIZE\tPATH")
            .map_err(|error| ArcthisError::io("writing list output", error))?;
        for entry in entries {
            writeln!(writer, "{}\t{}\t{}", entry.kind, entry.size, entry.path)
                .map_err(|error| ArcthisError::io("writing list output", error))?;
        }
        Ok(())
    }
}

pub(crate) fn write_tree(
    writer: &mut impl Write,
    path: &Path,
    format: ArchiveFormat,
    entries: &[ArchiveEntry],
    json: bool,
) -> Result<()> {
    let tree = build_tree(entries);
    if json {
        write_json(
            writer,
            &TreeResponse {
                schema_version: SCHEMA_VERSION,
                archive: ArchiveReference::new(path, format),
                tree: &tree,
            },
        )
    } else {
        writeln!(writer, "{}", path.display())
            .map_err(|error| ArcthisError::io("writing tree output", error))?;
        write_human_tree(writer, &tree, "")
    }
}

pub(crate) fn write_stat(
    writer: &mut impl Write,
    path: &Path,
    format: ArchiveFormat,
    entry: &ArchiveEntry,
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &StatResponse {
                schema_version: SCHEMA_VERSION,
                archive: ArchiveReference::new(path, format),
                entry,
            },
        )
    } else {
        writeln!(writer, "path: {}", entry.path)
            .and_then(|()| writeln!(writer, "kind: {}", entry.kind))
            .and_then(|()| writeln!(writer, "size: {}", entry.size))
            .and_then(|()| {
                writeln!(
                    writer,
                    "compressed size: {}",
                    entry
                        .compressed_size
                        .map_or_else(|| "unknown".to_owned(), |size| size.to_string())
                )
            })
            .and_then(|()| {
                writeln!(
                    writer,
                    "modified: {}",
                    entry.modified_time.as_deref().unwrap_or("unknown")
                )
            })
            .map_err(|error| ArcthisError::io("writing stat output", error))
    }
}

pub(crate) fn write_inspect(
    writer: &mut impl Write,
    path: &Path,
    format: ArchiveFormat,
    inspection: &ArchiveInspection,
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &InspectResponse {
                schema_version: SCHEMA_VERSION,
                archive: ArchiveReference::new(path, format),
                inspection,
            },
        )
    } else {
        writeln!(writer, "archive: {}", path.display())
            .and_then(|()| writeln!(writer, "format: {format}"))
            .and_then(|()| writeln!(writer, "compression: {}", inspection.compression))
            .and_then(|()| writeln!(writer, "entries: {}", inspection.entry_count))
            .and_then(|()| writeln!(writer, "compressed size: {}", inspection.compressed_size))
            .and_then(|()| {
                writeln!(
                    writer,
                    "uncompressed size: {}",
                    inspection.uncompressed_size
                )
            })
            .and_then(|()| writeln!(writer, "random access: {}", inspection.random_access))
            .and_then(|()| writeln!(writer, "multipart: {}", inspection.multipart))
            .and_then(|()| writeln!(writer, "volumes: {}", inspection.volume_count))
            .map_err(|error| ArcthisError::io("writing inspect output", error))?;
        for warning in &inspection.warnings {
            writeln!(writer, "warning [{}]: {}", warning.code, warning.message)
                .map_err(|error| ArcthisError::io("writing inspect output", error))?;
        }
        Ok(())
    }
}

pub(crate) fn write_extract(
    writer: &mut impl Write,
    path: &Path,
    format: ArchiveFormat,
    extraction: &ExtractResult,
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &ExtractResponse {
                schema_version: SCHEMA_VERSION,
                archive: ArchiveReference::new(path, format),
                extraction,
            },
        )
    } else {
        writeln!(
            writer,
            "extracted {} entries ({} bytes) to {}",
            extraction.entries_extracted,
            extraction.bytes_written,
            extraction.destination.display()
        )
        .map_err(|error| ArcthisError::io("writing extraction result", error))
    }
}

pub(crate) fn write_extract_plan(
    writer: &mut impl Write,
    plan: &ExtractPlan,
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &ExtractPlanResponse {
                schema_version: SCHEMA_VERSION,
                operation: "extract",
                plan,
            },
        )
    } else {
        writeln!(writer, "dry-run extraction plan")
            .and_then(|()| writeln!(writer, "source: {}", plan.source.display()))
            .and_then(|()| writeln!(writer, "destination: {}", plan.destination.display()))
            .and_then(|()| writeln!(writer, "entries: {}", plan.entries_to_extract))
            .and_then(|()| {
                writeln!(
                    writer,
                    "estimated bytes: {}",
                    plan.estimated_uncompressed_size
                )
            })
            .and_then(|()| writeln!(writer, "collision: {}", plan.collision))
            .and_then(|()| writeln!(writer, "will skip: {}", plan.will_skip))
            .and_then(|()| {
                writeln!(
                    writer,
                    "delete source after success: {}",
                    plan.will_delete_source_after_success
                )
            })
            .map_err(|error| ArcthisError::io("writing extraction plan", error))?;
        for warning in &plan.warnings {
            writeln!(writer, "warning: {warning}")
                .map_err(|error| ArcthisError::io("writing extraction plan", error))?;
        }
        Ok(())
    }
}

pub(crate) fn write_extract_all_plan(
    writer: &mut impl Write,
    plan: &ExtractAllPlan,
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &ExtractAllPlanResponse {
                schema_version: SCHEMA_VERSION,
                operation: "extract_all",
                plan,
            },
        )
    } else {
        writeln!(writer, "dry-run extract-all plan")
            .and_then(|()| writeln!(writer, "root: {}", plan.root.display()))
            .and_then(|()| writeln!(writer, "archives: {}", plan.archives.len()))
            .and_then(|()| writeln!(writer, "workers: {}", plan.workers))
            .map_err(|error| ArcthisError::io("writing extract-all plan", error))?;
        for archive in &plan.archives {
            writeln!(
                writer,
                "{} -> {}{}",
                archive.source.display(),
                archive.destination.display(),
                if archive.will_skip { " (skip)" } else { "" }
            )
            .map_err(|error| ArcthisError::io("writing extract-all plan", error))?;
        }
        for conflict in &plan.destination_conflicts {
            writeln!(writer, "conflict: {conflict}")
                .map_err(|error| ArcthisError::io("writing extract-all plan", error))?;
        }
        Ok(())
    }
}

pub(crate) fn write_extract_all(
    writer: &mut impl Write,
    result: &ExtractAllResult,
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &ExtractAllResponse {
                schema_version: SCHEMA_VERSION,
                operation: "extract_all",
                result,
            },
        )
    } else {
        writeln!(
            writer,
            "processed {} archives: {} completed, {} skipped, {} failed",
            result.discovered, result.succeeded, result.skipped, result.failed
        )
        .map_err(|error| ArcthisError::io("writing extract-all result", error))?;
        for item in &result.items {
            if let Some(error) = &item.error_message {
                writeln!(writer, "failed {}: {error}", item.archive.display())
            } else {
                writeln!(
                    writer,
                    "{:?} {} -> {}",
                    item.status,
                    item.archive.display(),
                    item.destination
                        .as_deref()
                        .map_or_else(|| "-".to_owned(), |path| path.display().to_string())
                )
            }
            .map_err(|error| ArcthisError::io("writing extract-all result", error))?;
        }
        Ok(())
    }
}

pub(crate) fn write_verify(
    writer: &mut impl Write,
    path: &Path,
    format: ArchiveFormat,
    verification: &VerificationResult,
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &VerifyResponse {
                schema_version: SCHEMA_VERSION,
                archive: ArchiveReference::new(path, format),
                verification,
            },
        )
    } else {
        writeln!(
            writer,
            "verified {} entries ({} bytes)",
            verification.entries_checked, verification.bytes_checked
        )
        .map_err(|error| ArcthisError::io("writing verification result", error))
    }
}

pub(crate) fn write_pack(writer: &mut impl Write, result: &PackResult, json: bool) -> Result<()> {
    if json {
        write_json(
            writer,
            &PackResponse {
                schema_version: SCHEMA_VERSION,
                pack: result,
            },
        )
    } else {
        writeln!(
            writer,
            "packed {} entries into {} ({} bytes, verified)",
            result.entries_packed,
            result.destination.display(),
            result.archive_size
        )
        .map_err(|error| ArcthisError::io("writing pack result", error))
    }
}

pub(crate) fn write_pack_plan(writer: &mut impl Write, plan: &PackPlan, json: bool) -> Result<()> {
    if json {
        write_json(
            writer,
            &PackPlanResponse {
                schema_version: SCHEMA_VERSION,
                operation: "pack",
                plan,
            },
        )
    } else {
        writeln!(writer, "dry-run pack plan")
            .and_then(|()| writeln!(writer, "source: {}", plan.source.display()))
            .and_then(|()| writeln!(writer, "destination: {}", plan.destination.display()))
            .and_then(|()| writeln!(writer, "format: {}", plan.format))
            .and_then(|()| writeln!(writer, "entries: {}", plan.entries_to_pack))
            .and_then(|()| writeln!(writer, "estimated bytes: {}", plan.estimated_input_size))
            .and_then(|()| writeln!(writer, "collision: {}", plan.collision))
            .and_then(|()| writeln!(writer, "will skip: {}", plan.will_skip))
            .and_then(|()| {
                writeln!(
                    writer,
                    "delete source after success: {}",
                    plan.will_delete_source_after_success
                )
            })
            .map_err(|error| ArcthisError::io("writing pack plan", error))
    }
}

pub(crate) fn write_index(writer: &mut impl Write, result: &IndexResult, json: bool) -> Result<()> {
    if json {
        write_json(
            writer,
            &IndexResponse {
                schema_version: SCHEMA_VERSION,
                operation: "index",
                index: result,
            },
        )
    } else {
        writeln!(
            writer,
            "{:?} index {} ({} entries) at {}",
            result.action,
            result.archive.display(),
            result.entries_indexed,
            result.index_path.display()
        )
        .map_err(|error| ArcthisError::io("writing index result", error))
    }
}

pub(crate) fn write_convert(
    writer: &mut impl Write,
    result: &ConvertResult,
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &ConvertResponse {
                schema_version: SCHEMA_VERSION,
                convert: result,
            },
        )
    } else {
        writeln!(
            writer,
            "converted {} entries to {} ({} bytes, verified)",
            result.entries_converted,
            result.destination.display(),
            result.archive_size
        )
        .map_err(|error| ArcthisError::io("writing conversion result", error))
    }
}

pub(crate) fn write_convert_plan(
    writer: &mut impl Write,
    plan: &ConvertPlan,
    json: bool,
) -> Result<()> {
    if json {
        write_json(
            writer,
            &ConvertPlanResponse {
                schema_version: SCHEMA_VERSION,
                operation: "convert",
                plan,
            },
        )
    } else {
        writeln!(writer, "dry-run conversion plan")
            .and_then(|()| writeln!(writer, "source: {}", plan.source.display()))
            .and_then(|()| writeln!(writer, "source format: {}", plan.source_format))
            .and_then(|()| writeln!(writer, "destination: {}", plan.destination.display()))
            .and_then(|()| writeln!(writer, "target format: {}", plan.target_format))
            .and_then(|()| writeln!(writer, "entries: {}", plan.entries_to_convert))
            .and_then(|()| writeln!(writer, "collision: {}", plan.collision))
            .and_then(|()| writeln!(writer, "will skip: {}", plan.will_skip))
            .and_then(|()| {
                writeln!(
                    writer,
                    "delete source after success: {}",
                    plan.will_delete_source_after_success
                )
            })
            .map_err(|error| ArcthisError::io("writing conversion plan", error))
    }
}

fn write_json(writer: &mut impl Write, value: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *writer, value)
        .map_err(|error| ArcthisError::io("serializing JSON output", io::Error::other(error)))?;
    writeln!(writer).map_err(|error| ArcthisError::io("writing JSON output", error))
}

fn build_tree(entries: &[ArchiveEntry]) -> Vec<TreeNode> {
    let mut roots = Vec::new();
    for entry in entries {
        let components = entry
            .path
            .split('/')
            .filter(|component| !component.is_empty())
            .collect::<Vec<_>>();
        if components.is_empty() {
            continue;
        }
        insert_entry(&mut roots, &components, entry, "");
    }
    sort_tree(&mut roots);
    roots
}

fn insert_entry(
    nodes: &mut Vec<TreeNode>,
    components: &[&str],
    entry: &ArchiveEntry,
    parent: &str,
) {
    let name = components[0];
    let path = if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    };

    if components.len() == 1 {
        if entry.kind == EntryKind::Directory
            && let Some(node) = nodes
                .iter_mut()
                .find(|node| node.name == name && node.kind == EntryKind::Directory)
        {
            node.entry = Some(entry.clone());
            return;
        }
        nodes.push(TreeNode {
            name: name.to_owned(),
            path,
            kind: entry.kind,
            entry: Some(entry.clone()),
            children: Vec::new(),
        });
        return;
    }

    let directory_index = nodes
        .iter()
        .position(|node| node.name == name && node.kind == EntryKind::Directory)
        .unwrap_or_else(|| {
            nodes.push(TreeNode {
                name: name.to_owned(),
                path: path.clone(),
                kind: EntryKind::Directory,
                entry: None,
                children: Vec::new(),
            });
            nodes.len() - 1
        });
    insert_entry(
        &mut nodes[directory_index].children,
        &components[1..],
        entry,
        &path,
    );
}

fn sort_tree(nodes: &mut [TreeNode]) {
    nodes.sort_by(|left, right| left.name.cmp(&right.name));
    for node in nodes {
        sort_tree(&mut node.children);
    }
}

fn write_human_tree(writer: &mut impl Write, nodes: &[TreeNode], prefix: &str) -> Result<()> {
    for (index, node) in nodes.iter().enumerate() {
        let last = index + 1 == nodes.len();
        let connector = if last { "└── " } else { "├── " };
        writeln!(writer, "{prefix}{connector}{}", node.name)
            .map_err(|error| ArcthisError::io("writing tree output", error))?;
        let child_prefix = if last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };
        write_human_tree(writer, &node.children, &child_prefix)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::build_tree;
    use crate::model::{ArchiveEntry, EntryKind};

    fn entry(path: &str, kind: EntryKind) -> ArchiveEntry {
        ArchiveEntry {
            archive_index: 0,
            path: path.to_owned(),
            path_encoding: crate::EntryPathEncoding::Utf8,
            kind,
            size: 0,
            compressed_size: None,
            modified_time: None,
            encrypted: false,
            executable: false,
            symlink_target: None,
            crc32: None,
            mime_guess: None,
        }
    }

    #[test]
    fn tree_merges_implicit_and_explicit_directories() {
        let entries = vec![
            entry("src/lib.rs", EntryKind::File),
            entry("src/", EntryKind::Directory),
        ];
        let tree = build_tree(&entries);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "src");
        assert!(tree[0].entry.is_some());
        assert_eq!(tree[0].children[0].name, "lib.rs");
    }

    #[test]
    fn tree_preserves_duplicate_file_entries() {
        let entries = vec![
            entry("same.txt", EntryKind::File),
            entry("same.txt", EntryKind::File),
        ];
        assert_eq!(build_tree(&entries).len(), 2);
    }
}
