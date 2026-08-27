use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tempfile::Builder;

use crate::archive::{Archive, ArchiveOpenOptions};
use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractOptions, enforce_entry_count, validate_entries};
use crate::lifecycle::{
    CollisionPolicy, OperationStatus, delete_source, ensure_executable_resolution,
    resolve_destination,
};
use crate::model::{ArchiveFormat, VerificationResult};
use crate::pack::{PackOptions, output_format, pack_source_with_options};
use crate::security::ExtractionLimits;

#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub open: ArchiveOpenOptions,
    pub limits: ExtractionLimits,
    pub collision_policy: CollisionPolicy,
    pub delete_source: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            open: ArchiveOpenOptions::default(),
            limits: ExtractionLimits::default(),
            collision_policy: CollisionPolicy::Refuse,
            delete_source: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct ConvertPlan {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_format: ArchiveFormat,
    pub target_format: ArchiveFormat,
    pub access_strategy: &'static str,
    pub entries_to_convert: u64,
    pub estimated_uncompressed_size: u64,
    pub collision: bool,
    pub collision_policy: CollisionPolicy,
    pub will_skip: bool,
    pub will_overwrite: bool,
    pub renamed_destination: bool,
    pub will_delete_source_after_success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConvertResult {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_format: ArchiveFormat,
    pub target_format: ArchiveFormat,
    pub entries_converted: u64,
    pub archive_size: u64,
    pub verification: VerificationResult,
    pub status: OperationStatus,
    pub source_deleted: bool,
}

pub fn plan_convert(source: &Path, output: &Path, options: &ConvertOptions) -> Result<ConvertPlan> {
    let source = fs::canonicalize(source)
        .map_err(|error| ArcthisError::io("resolving conversion source", error))?;
    reject_same_source_and_destination(&source, output)?;
    let target_format = output_format(output)?;
    let archive = Archive::open_with_options(source.as_path(), &options.open)?;
    let entries = archive.entries()?;
    enforce_entry_count(entries.len(), &options.limits)?;
    let validated = validate_entries(&entries, &options.limits)?;
    validate_target_shape(&entries, target_format)?;
    let estimated_uncompressed_size = validated.iter().try_fold(0_u64, |total, (entry, _)| {
        total
            .checked_add(entry.size)
            .ok_or_else(|| ArcthisError::ResourceLimit {
                message: "conversion input size overflows u64".to_owned(),
            })
    })?;
    let resolution = resolve_destination(output, options.collision_policy)?;
    Ok(ConvertPlan {
        source,
        destination: resolution.path,
        source_format: archive.format(),
        target_format,
        access_strategy: "staged_materialization",
        entries_to_convert: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        estimated_uncompressed_size,
        collision: resolution.existed,
        collision_policy: options.collision_policy,
        will_skip: resolution.skip,
        will_overwrite: resolution.existed
            && options.collision_policy == CollisionPolicy::Overwrite,
        renamed_destination: resolution.renamed,
        will_delete_source_after_success: options.delete_source && !resolution.skip,
    })
}

fn validate_target_shape(entries: &[crate::ArchiveEntry], target: ArchiveFormat) -> Result<()> {
    if !matches!(
        target,
        ArchiveFormat::Gzip | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd
    ) {
        return Ok(());
    }
    let mut files = entries
        .iter()
        .filter(|entry| entry.kind == crate::EntryKind::File);
    let Some(file) = files.next() else {
        return Err(ArcthisError::UnsupportedOperation {
            message: format!("{target} conversion requires exactly one regular file"),
        });
    };
    if files.next().is_some()
        || entries
            .iter()
            .any(|entry| entry.kind != crate::EntryKind::File)
        || file.path.split('/').filter(|part| !part.is_empty()).count() != 1
    {
        return Err(ArcthisError::UnsupportedOperation {
            message: format!(
                "{target} conversion requires one root-level regular file and no other entries"
            ),
        });
    }
    Ok(())
}

pub fn convert_archive(
    source: &Path,
    output: &Path,
    options: &ConvertOptions,
) -> Result<ConvertResult> {
    if options.delete_source && !options.open.volumes.is_empty() {
        return Err(ArcthisError::UnsupportedOperation {
            message: "--delete-source is not supported for multipart conversion".to_owned(),
        });
    }
    let plan = plan_convert(source, output, options)?;
    let resolution = resolve_destination(output, options.collision_policy)?;
    ensure_executable_resolution(&resolution, options.collision_policy)?;
    if resolution.skip {
        let existing = Archive::open(resolution.path.as_path())?;
        let verification = existing.verify()?;
        let archive_size = fs::metadata(&resolution.path)
            .map_err(|error| ArcthisError::io("reading existing conversion output", error))?
            .len();
        return Ok(ConvertResult {
            source: plan.source,
            destination: resolution.path,
            source_format: plan.source_format,
            target_format: existing.format(),
            entries_converted: 0,
            archive_size,
            verification,
            status: OperationStatus::Skipped,
            source_deleted: false,
        });
    }

    let staging = Builder::new()
        .prefix("arcthis-convert-")
        .tempdir()
        .map_err(|error| ArcthisError::io("creating conversion staging directory", error))?;
    let payload = staging.path().join("payload");
    let archive = Archive::open_with_options(plan.source.as_path(), &options.open)?;
    let extraction = archive.extract(
        None,
        &ExtractOptions {
            output: Some(payload.clone()),
            base_directory: None,
            limits: options.limits,
            collision_policy: CollisionPolicy::Refuse,
            delete_source: false,
        },
    )?;
    let packed = pack_source_with_options(
        &payload,
        &resolution.path,
        &PackOptions {
            collision_policy: options.collision_policy,
            delete_source: false,
            include_source_root: false,
        },
    )?;
    let source_deleted = if options.delete_source {
        delete_source(&plan.source)?;
        true
    } else {
        false
    };
    Ok(ConvertResult {
        source: plan.source,
        destination: packed.destination,
        source_format: plan.source_format,
        target_format: packed.format,
        entries_converted: extraction.entries_extracted,
        archive_size: packed.archive_size,
        verification: packed.verification,
        status: packed.status,
        source_deleted,
    })
}

fn reject_same_source_and_destination(source: &Path, output: &Path) -> Result<()> {
    let output_absolute = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| ArcthisError::io("reading current directory", error))?
            .join(output)
    };
    if output_absolute == source
        || output_absolute
            .canonicalize()
            .is_ok_and(|canonical| canonical == source)
    {
        return Err(ArcthisError::Collision {
            message: "conversion source and destination must be different paths".to_owned(),
        });
    }
    Ok(())
}
