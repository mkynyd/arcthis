mod backend;
mod detect;
mod locator;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use backend::{ArchiveBackend, TarBackend, ZipBackend};

use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractOptions, ExtractResult, ExtractionStats, PlannedEntry};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, ArchiveInspection, ArchiveWarning,
    EntryCopyResult, EntryKind, EntryPathEncoding, VerificationResult,
};
use crate::security::{ExtractionLimits, validate_entry_path};

pub use locator::ArchiveLocator;

/// Format-independent access to one archive source.
pub struct Archive {
    locator: ArchiveLocator,
    backend: Box<dyn ArchiveBackend>,
}

impl Archive {
    pub fn open(locator: impl Into<ArchiveLocator>) -> Result<Self> {
        let locator = locator.into();
        let format = detect::detect(&locator)?;
        let backend: Box<dyn ArchiveBackend> = match format {
            ArchiveFormat::Zip => Box::new(ZipBackend::new(locator.path().to_path_buf())),
            ArchiveFormat::Tar | ArchiveFormat::TarGzip => {
                Box::new(TarBackend::new(locator.path().to_path_buf(), format))
            }
        };
        backend.validate()?;
        Ok(Self { locator, backend })
    }

    pub fn path(&self) -> &Path {
        self.locator.path()
    }

    pub fn format(&self) -> ArchiveFormat {
        self.backend.format()
    }

    pub fn capabilities(&self) -> ArchiveCapabilities {
        self.backend.capabilities()
    }

    pub fn entries(&self) -> Result<Vec<ArchiveEntry>> {
        self.backend.entries()
    }

    pub fn entry(&self, path: &str) -> Result<ArchiveEntry> {
        let mut matches = self
            .entries()?
            .into_iter()
            .filter(|entry| entry.path == path);
        let entry = matches.next().ok_or_else(|| ArcthisError::EntryNotFound {
            entry: path.to_owned(),
        })?;
        if matches.next().is_some() {
            return Err(ArcthisError::Collision {
                message: format!("entry path `{path}` occurs more than once"),
            });
        }
        Ok(entry)
    }

    pub fn copy_entry_to(&self, path: &str, writer: &mut dyn Write) -> Result<EntryCopyResult> {
        let entry = self.entry(path)?;
        if entry.kind != EntryKind::File {
            return Err(ArcthisError::UnsupportedOperation {
                message: format!("entry `{path}` is not a regular file"),
            });
        }
        if entry.encrypted {
            return Err(ArcthisError::PasswordRequired);
        }
        self.backend.copy_entry_to(path, writer)
    }

    pub fn inspect(&self) -> Result<ArchiveInspection> {
        let entries = self.entries()?;
        let compressed_size = fs::metadata(self.path())
            .map_err(|error| ArcthisError::io("reading archive metadata", error))?
            .len();
        let mut uncompressed_size = 0_u64;
        let mut size_overflow = false;
        for entry in &entries {
            if let Some(size) = uncompressed_size.checked_add(entry.size) {
                uncompressed_size = size;
            } else {
                uncompressed_size = u64::MAX;
                size_overflow = true;
            }
        }

        let capabilities = self.capabilities();
        let encrypted = entries.iter().any(|entry| entry.encrypted);
        let mut warnings = Vec::new();
        if !capabilities.random_access {
            warnings.push(ArchiveWarning {
                code: "sequential_access".to_owned(),
                message: "selected entry access may require a sequential archive scan".to_owned(),
            });
        }
        if encrypted {
            warnings.push(ArchiveWarning {
                code: "encrypted_entries_unsupported".to_owned(),
                message: "this build cannot read encrypted entries".to_owned(),
            });
        }
        if entries.iter().any(|entry| {
            matches!(
                entry.kind,
                EntryKind::Symlink | EntryKind::Hardlink | EntryKind::Other
            )
        }) {
            warnings.push(ArchiveWarning {
                code: "non_regular_entries".to_owned(),
                message: "archive contains links or special entry kinds that extraction rejects"
                    .to_owned(),
            });
        }
        let mut counts = HashMap::new();
        for entry in &entries {
            *counts.entry(entry.path.as_str()).or_insert(0_u64) += 1;
        }
        if counts.values().any(|count| *count > 1) {
            warnings.push(ArchiveWarning {
                code: "duplicate_entry_paths".to_owned(),
                message: "archive contains duplicate entry paths; named access is ambiguous"
                    .to_owned(),
            });
        }
        if size_overflow {
            warnings.push(ArchiveWarning {
                code: "size_overflow".to_owned(),
                message: "declared uncompressed sizes exceed the representable total".to_owned(),
            });
        }
        let default_limits = ExtractionLimits::default();
        if entries.iter().any(|entry| {
            entry.path_encoding != EntryPathEncoding::Utf8
                || validate_entry_path(&entry.path, entry.kind, &default_limits).is_err()
        }) {
            warnings.push(ArchiveWarning {
                code: "unsafe_entry_paths".to_owned(),
                message: "one or more entry paths would be rejected by safe extraction".to_owned(),
            });
        }
        let exceeds_default_limits = entry_count_exceeds_default(&entries, &default_limits);
        if exceeds_default_limits {
            warnings.push(ArchiveWarning {
                code: "default_extraction_limits_exceeded".to_owned(),
                message: "declared archive metadata exceeds default extraction limits".to_owned(),
            });
        }

        let compression_ratio =
            calculate_compression_ratio(compressed_size, uncompressed_size, size_overflow);
        let entry_count = u64::try_from(entries.len()).unwrap_or(u64::MAX);
        Ok(ArchiveInspection {
            compression: match self.format() {
                ArchiveFormat::Zip => "mixed".to_owned(),
                ArchiveFormat::Tar => "none".to_owned(),
                ArchiveFormat::TarGzip => "gzip".to_owned(),
            },
            encrypted,
            solid: capabilities.solid,
            random_access: capabilities.random_access,
            entry_count,
            compressed_size,
            uncompressed_size,
            compression_ratio,
            warnings,
            capabilities,
        })
    }

    pub fn extract(
        &self,
        selected_entry: Option<&str>,
        options: &ExtractOptions,
    ) -> Result<ExtractResult> {
        crate::extract::extract_archive(self, selected_entry, options)
    }

    pub(crate) fn extract_plan(
        &self,
        plan: &[PlannedEntry],
        staging_root: &Path,
        limits: &crate::ExtractionLimits,
    ) -> Result<ExtractionStats> {
        self.backend.extract_plan(plan, staging_root, limits)
    }

    pub fn verify(&self) -> Result<VerificationResult> {
        self.backend.verify()
    }
}

fn entry_count_exceeds_default(entries: &[ArchiveEntry], limits: &ExtractionLimits) -> bool {
    let count = u64::try_from(entries.len()).unwrap_or(u64::MAX);
    if count > limits.max_entries {
        return true;
    }
    let mut total = 0_u64;
    for entry in entries.iter().filter(|entry| entry.kind == EntryKind::File) {
        if entry.size > limits.max_entry_size {
            return true;
        }
        let Some(next) = total.checked_add(entry.size) else {
            return true;
        };
        total = next;
        if total > limits.max_total_size {
            return true;
        }
    }
    false
}

#[allow(clippy::cast_precision_loss)] // JSON ratios are approximate by definition.
fn calculate_compression_ratio(
    compressed_size: u64,
    uncompressed_size: u64,
    size_overflow: bool,
) -> Option<f64> {
    if uncompressed_size == 0 || size_overflow {
        None
    } else {
        Some(compressed_size as f64 / uncompressed_size as f64)
    }
}
