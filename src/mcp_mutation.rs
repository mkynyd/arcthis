//! Controlled MCP mutation planning, binding, and execution.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::CancellationToken;
use crate::archive::{Archive, ArchiveOpenOptions};
use crate::convert::{ConvertOptions, ConvertPlan, ConvertResult};
use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractOptions, ExtractPlan, ExtractResult};
use crate::lifecycle::CollisionPolicy;
use crate::pack::{PackOptions, PackPlan, PackResult};
use crate::security::ExtractionLimits;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MutationCollision {
    #[default]
    Refuse,
    Overwrite,
    SkipExisting,
    Rename,
}

impl From<MutationCollision> for CollisionPolicy {
    fn from(value: MutationCollision) -> Self {
        match value {
            MutationCollision::Refuse => Self::Refuse,
            MutationCollision::Overwrite => Self::Overwrite,
            MutationCollision::SkipExisting => Self::SkipExisting,
            MutationCollision::Rename => Self::Rename,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExtractionMutationInput {
    pub path: String,
    pub output: String,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default = "default_max_entries")]
    pub max_entries: u64,
    #[serde(default = "default_max_total_size")]
    pub max_total_size: u64,
    #[serde(default = "default_max_entry_size")]
    pub max_entry_size: u64,
    #[serde(default)]
    pub max_compression_ratio: Option<u64>,
    #[serde(default)]
    pub collision: MutationCollision,
    #[serde(default)]
    pub delete_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct PackMutationInput {
    pub path: String,
    pub output: String,
    #[serde(default)]
    pub collision: MutationCollision,
    #[serde(default)]
    pub delete_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConvertMutationInput {
    pub path: String,
    pub output: String,
    #[serde(default = "default_max_entries")]
    pub max_entries: u64,
    #[serde(default = "default_max_total_size")]
    pub max_total_size: u64,
    #[serde(default = "default_max_entry_size")]
    pub max_entry_size: u64,
    #[serde(default)]
    pub max_compression_ratio: Option<u64>,
    #[serde(default)]
    pub collision: MutationCollision,
    #[serde(default)]
    pub delete_source: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ExtractExecuteInput {
    #[serde(flatten)]
    pub request: ExtractionMutationInput,
    pub plan_digest: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct PackExecuteInput {
    #[serde(flatten)]
    pub request: PackMutationInput,
    pub plan_digest: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConvertExecuteInput {
    #[serde(flatten)]
    pub request: ConvertMutationInput,
    pub plan_digest: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ExtractPlanOutput {
    pub plan_digest: String,
    pub plan: ExtractPlan,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct PackPlanOutput {
    pub plan_digest: String,
    pub plan: PackPlan,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConvertPlanOutput {
    pub plan_digest: String,
    pub plan: ConvertPlan,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ExtractExecuteOutput {
    pub plan_digest: String,
    pub result: ExtractResult,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct PackExecuteOutput {
    pub plan_digest: String,
    pub result: PackResult,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConvertExecuteOutput {
    pub plan_digest: String,
    pub result: ConvertResult,
}

pub(crate) fn plan_extract(
    source: &Path,
    output: &Path,
    request: &ExtractionMutationInput,
) -> Result<ExtractPlanOutput> {
    let archive = Archive::open(source)?;
    let options = extract_options(output, request);
    let plan = archive.plan_extract(request.entry.as_deref(), &options)?;
    let plan_digest = plan_digest("extract", request, &plan, &plan.source, &plan.destination)?;
    Ok(ExtractPlanOutput { plan_digest, plan })
}

pub(crate) fn execute_extract(
    source: &Path,
    output: &Path,
    input: &ExtractExecuteInput,
    cancellation: &CancellationToken,
) -> Result<ExtractExecuteOutput> {
    cancellation.checkpoint()?;
    let prepared = plan_extract(source, output, &input.request)?;
    cancellation.checkpoint()?;
    ensure_digest(&input.plan_digest, &prepared.plan_digest)?;
    let archive = Archive::open(source)?;
    let result = archive.extract(
        input.request.entry.as_deref(),
        &extract_options(output, &input.request),
    )?;
    Ok(ExtractExecuteOutput {
        plan_digest: prepared.plan_digest,
        result,
    })
}

pub(crate) fn plan_pack(
    source: &Path,
    output: &Path,
    request: &PackMutationInput,
) -> Result<PackPlanOutput> {
    let options = pack_options(request);
    let plan = crate::pack::plan_pack_source(source, output, &options)?;
    let plan_digest = plan_digest("pack", request, &plan, &plan.source, &plan.destination)?;
    Ok(PackPlanOutput { plan_digest, plan })
}

pub(crate) fn execute_pack(
    source: &Path,
    output: &Path,
    input: &PackExecuteInput,
    cancellation: &CancellationToken,
) -> Result<PackExecuteOutput> {
    cancellation.checkpoint()?;
    let prepared = plan_pack(source, output, &input.request)?;
    cancellation.checkpoint()?;
    ensure_digest(&input.plan_digest, &prepared.plan_digest)?;
    let result =
        crate::pack::pack_source_with_options(source, output, &pack_options(&input.request))?;
    Ok(PackExecuteOutput {
        plan_digest: prepared.plan_digest,
        result,
    })
}

pub(crate) fn plan_convert(
    source: &Path,
    output: &Path,
    request: &ConvertMutationInput,
) -> Result<ConvertPlanOutput> {
    let options = convert_options(request);
    let plan = crate::convert::plan_convert(source, output, &options)?;
    let plan_digest = plan_digest("convert", request, &plan, &plan.source, &plan.destination)?;
    Ok(ConvertPlanOutput { plan_digest, plan })
}

pub(crate) fn execute_convert(
    source: &Path,
    output: &Path,
    input: &ConvertExecuteInput,
    cancellation: &CancellationToken,
) -> Result<ConvertExecuteOutput> {
    cancellation.checkpoint()?;
    let prepared = plan_convert(source, output, &input.request)?;
    cancellation.checkpoint()?;
    ensure_digest(&input.plan_digest, &prepared.plan_digest)?;
    let result = crate::convert::convert_archive(source, output, &convert_options(&input.request))?;
    Ok(ConvertExecuteOutput {
        plan_digest: prepared.plan_digest,
        result,
    })
}

fn extract_options(output: &Path, request: &ExtractionMutationInput) -> ExtractOptions {
    ExtractOptions {
        output: Some(output.to_path_buf()),
        base_directory: None,
        limits: extraction_limits(
            request.max_entries,
            request.max_total_size,
            request.max_entry_size,
            request.max_compression_ratio,
        ),
        collision_policy: request.collision.into(),
        delete_source: request.delete_source,
    }
}

fn pack_options(request: &PackMutationInput) -> PackOptions {
    PackOptions {
        collision_policy: request.collision.into(),
        delete_source: request.delete_source,
        include_source_root: true,
    }
}

fn convert_options(request: &ConvertMutationInput) -> ConvertOptions {
    ConvertOptions {
        open: ArchiveOpenOptions::default(),
        limits: extraction_limits(
            request.max_entries,
            request.max_total_size,
            request.max_entry_size,
            request.max_compression_ratio,
        ),
        collision_policy: request.collision.into(),
        delete_source: request.delete_source,
    }
}

fn extraction_limits(
    max_entries: u64,
    max_total_size: u64,
    max_entry_size: u64,
    max_compression_ratio: Option<u64>,
) -> ExtractionLimits {
    ExtractionLimits {
        max_entries,
        max_total_size,
        max_entry_size,
        max_compression_ratio,
        ..ExtractionLimits::default()
    }
}

fn ensure_digest(expected: &str, actual: &str) -> Result<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(ArcthisError::Collision {
            message:
                "stale MCP mutation plan: source, destination, limits, or collision state changed"
                    .to_owned(),
        })
    }
}

fn plan_digest<R: Serialize, P: Serialize>(
    operation: &str,
    request: &R,
    plan: &P,
    source: &Path,
    destination: &Path,
) -> Result<String> {
    let mut digest = Sha256::new();
    digest.update(b"arcthis-mcp-plan-v1\0");
    digest.update(operation.as_bytes());
    digest.update(b"\0request\0");
    update_json(&mut digest, request)?;
    digest.update(b"\0plan\0");
    update_json(&mut digest, plan)?;
    digest.update(b"\0source\0");
    fingerprint_path(source, &mut digest)?;
    digest.update(b"\0destination\0");
    if destination
        .try_exists()
        .map_err(|error| ArcthisError::io("checking MCP plan destination fingerprint", error))?
    {
        fingerprint_path(destination, &mut digest)?;
    } else {
        digest.update(b"missing\0");
        digest.update(destination.as_os_str().as_encoded_bytes());
    }
    Ok(hex(&digest.finalize()))
}

fn update_json(digest: &mut Sha256, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|error| ArcthisError::UnsupportedOperation {
        message: format!("serializing MCP mutation plan binding: {error}"),
    })?;
    digest.update(bytes);
    Ok(())
}

fn fingerprint_path(path: &Path, digest: &mut Sha256) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ArcthisError::io("reading MCP plan source metadata", error))?;
    if metadata.file_type().is_symlink() {
        return Err(ArcthisError::UnsupportedOperation {
            message: format!(
                "MCP mutation fingerprints reject symlinks: {}",
                path.display()
            ),
        });
    }
    if metadata.is_file() {
        fingerprint_file(
            path,
            path.file_name().unwrap_or_default().as_encoded_bytes(),
            digest,
        )?;
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(ArcthisError::UnsupportedOperation {
            message: format!(
                "MCP mutation source is not a file or directory: {}",
                path.display()
            ),
        });
    }
    for entry in WalkDir::new(path).follow_links(false).sort_by_file_name() {
        let entry = entry.map_err(|error| {
            ArcthisError::io("walking MCP mutation source", std::io::Error::other(error))
        })?;
        let relative = entry.path().strip_prefix(path).map_err(|error| {
            ArcthisError::UnsupportedOperation {
                message: format!("deriving MCP fingerprint path: {error}"),
            }
        })?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| ArcthisError::io("reading MCP fingerprint metadata", error))?;
        if metadata.file_type().is_symlink() {
            return Err(ArcthisError::UnsupportedOperation {
                message: format!(
                    "MCP mutation fingerprints reject symlinks: {}",
                    entry.path().display()
                ),
            });
        }
        digest.update(relative.as_os_str().as_encoded_bytes());
        digest.update(b"\0");
        update_metadata(digest, &metadata);
        if metadata.is_file() {
            fingerprint_file(entry.path(), b"", digest)?;
        }
    }
    Ok(())
}

fn fingerprint_file(path: &Path, label: &[u8], digest: &mut Sha256) -> Result<()> {
    let metadata = fs::metadata(path)
        .map_err(|error| ArcthisError::io("reading MCP fingerprint file metadata", error))?;
    digest.update(label);
    digest.update(b"\0file\0");
    update_metadata(digest, &metadata);
    let mut file = File::open(path)
        .map_err(|error| ArcthisError::io("opening MCP fingerprint file", error))?;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| ArcthisError::io("hashing MCP fingerprint file", error))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(())
}

fn update_metadata(digest: &mut Sha256, metadata: &fs::Metadata) {
    digest.update(metadata.len().to_le_bytes());
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0_u128, |duration| duration.as_nanos());
    digest.update(modified.to_le_bytes());
    digest.update([u8::from(metadata.is_file()), u8::from(metadata.is_dir())]);
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(DIGITS[usize::from(byte >> 4)]));
        result.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    result
}

const fn default_max_entries() -> u64 {
    100_000
}

const fn default_max_total_size() -> u64 {
    16 * 1024 * 1024 * 1024
}

const fn default_max_entry_size() -> u64 {
    4 * 1024 * 1024 * 1024
}
