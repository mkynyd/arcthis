mod backend;
mod codec;
mod detect;
mod locator;

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use backend::{
    ArchiveBackend, ArchiveSource, RarBackend, SevenZipBackend, StreamBackend, TarBackend,
    ZipBackend,
};

use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractOptions, ExtractPlan, ExtractResult, ExtractionStats, PlannedEntry};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, ArchiveInspection, ArchiveWarning,
    EntryCopyResult, EntryKind, EntryPathEncoding, VerificationResult,
};
use crate::security::{ExtractionLimits, validate_entry_path};

pub use locator::ArchiveLocator;

#[derive(Clone)]
pub struct ArchivePassword(Arc<SecretBytes>);

struct SecretBytes(Vec<u8>);

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

impl std::fmt::Debug for ArchivePassword {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ArchivePassword([REDACTED])")
    }
}

impl ArchivePassword {
    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        Self(Arc::new(SecretBytes(value.into())))
    }

    pub fn expose(&self) -> &[u8] {
        &self.0.0
    }

    fn as_utf8(&self) -> Result<&str> {
        std::str::from_utf8(self.expose()).map_err(|_| ArcthisError::UnsupportedOperation {
            message: "this archive backend requires a UTF-8 password".to_owned(),
        })
    }
}

#[derive(Clone, Default)]
pub struct ArchiveOpenOptions {
    pub password: Option<ArchivePassword>,
    /// Remaining byte-stream volumes, in exact order after the primary path.
    pub volumes: Vec<PathBuf>,
    /// Override the platform cache root used by persistent archive indexes.
    pub index_directory: Option<PathBuf>,
}

impl std::fmt::Debug for ArchiveOpenOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ArchiveOpenOptions")
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("volumes", &self.volumes)
            .field("index_directory", &self.index_directory)
            .finish()
    }
}

/// Format-independent access to one archive source.
pub struct Archive {
    locator: ArchiveLocator,
    source: ArchiveSource,
    backend: Box<dyn ArchiveBackend>,
    entry_index: Mutex<Option<Vec<ArchiveEntry>>>,
    index_directory: Option<PathBuf>,
}

impl Archive {
    pub fn open(locator: impl Into<ArchiveLocator>) -> Result<Self> {
        Self::open_with_options(locator, &ArchiveOpenOptions::default())
    }

    pub fn open_with_options(
        locator: impl Into<ArchiveLocator>,
        options: &ArchiveOpenOptions,
    ) -> Result<Self> {
        let locator = locator.into();
        let source = if options.volumes.is_empty() {
            ArchiveSource::file(locator.path().to_path_buf())
        } else {
            let mut paths = Vec::with_capacity(options.volumes.len() + 1);
            paths.push(locator.path().to_path_buf());
            paths.extend(options.volumes.iter().cloned());
            let mut unique = std::collections::HashSet::with_capacity(paths.len());
            if paths.iter().any(|path| !unique.insert(path.clone())) {
                return Err(ArcthisError::Collision {
                    message: "multipart volume paths must be unique".to_owned(),
                });
            }
            ArchiveSource::multipart(paths)?
        };
        Self::open_source(locator, source, options)
    }

    pub fn open_within(
        locator: impl Into<ArchiveLocator>,
        entries: &[String],
        max_nested_entry_size: u64,
    ) -> Result<Self> {
        Self::open_within_options(
            locator,
            entries,
            max_nested_entry_size,
            &ArchiveOpenOptions::default(),
        )
    }

    pub fn open_within_options(
        locator: impl Into<ArchiveLocator>,
        entries: &[String],
        max_nested_entry_size: u64,
        options: &ArchiveOpenOptions,
    ) -> Result<Self> {
        const MAX_NESTED_DEPTH: usize = 8;
        if entries.len() > MAX_NESTED_DEPTH {
            return Err(ArcthisError::ResourceLimit {
                message: format!("nested archive depth exceeds {MAX_NESTED_DEPTH}"),
            });
        }
        let mut archive = Self::open_with_options(locator, options)?;
        for entry_path in entries {
            let entry = archive.entry(entry_path)?;
            if entry.size > max_nested_entry_size {
                return Err(ArcthisError::ResourceLimit {
                    message: format!(
                        "nested archive entry `{entry_path}` declares {} bytes, above limit {max_nested_entry_size}",
                        entry.size
                    ),
                });
            }
            let capacity =
                usize::try_from(entry.size).map_err(|_| ArcthisError::ResourceLimit {
                    message: format!("nested archive entry `{entry_path}` cannot fit in memory"),
                })?;
            let mut writer = BoundedMemoryWriter::new(capacity, max_nested_entry_size)?;
            let copy_result = archive.copy_entry_to(entry_path, &mut writer);
            if writer.exceeded {
                return Err(ArcthisError::ResourceLimit {
                    message: format!(
                        "nested archive entry `{entry_path}` exceeded {max_nested_entry_size} bytes while decoding"
                    ),
                });
            }
            copy_result?;
            let display = PathBuf::from(format!(
                "{}::{}",
                archive.path().to_string_lossy(),
                entry_path
            ));
            let locator = ArchiveLocator::file(display.clone());
            let source = ArchiveSource::memory(writer.bytes, display);
            archive = Self::open_source(locator, source, options)?;
        }
        Ok(archive)
    }

    fn open_source(
        locator: ArchiveLocator,
        source: ArchiveSource,
        options: &ArchiveOpenOptions,
    ) -> Result<Self> {
        let format = detect::detect(&source)?;
        let backend: Box<dyn ArchiveBackend> = match format {
            ArchiveFormat::Zip => {
                Box::new(ZipBackend::new(source.clone(), options.password.clone()))
            }
            ArchiveFormat::SevenZip => Box::new(SevenZipBackend::new(
                source.clone(),
                options.password.clone(),
            )?),
            ArchiveFormat::Rar => {
                Box::new(RarBackend::new(source.clone(), options.password.clone()))
            }
            ArchiveFormat::Tar
            | ArchiveFormat::TarGzip
            | ArchiveFormat::TarBzip2
            | ArchiveFormat::TarXz
            | ArchiveFormat::TarZstd => Box::new(TarBackend::new(source.clone(), format)),
            ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Xz
            | ArchiveFormat::Zstd => Box::new(StreamBackend::new(source.clone(), format)?),
        };
        backend.validate()?;
        Ok(Self {
            locator,
            source,
            backend,
            entry_index: Mutex::new(None),
            index_directory: options.index_directory.clone(),
        })
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
        if let Some(entries) = self
            .entry_index
            .lock()
            .map_err(|_| ArcthisError::UnsupportedOperation {
                message: "archive entry index lock was poisoned".to_owned(),
            })?
            .as_ref()
        {
            return Ok(entries.clone());
        }
        if let Some(path) = self.source.file_path()
            && let Some(entries) = crate::index::load_cached_entries(
                path,
                self.format(),
                self.index_directory.as_deref(),
            )?
        {
            *self
                .entry_index
                .lock()
                .map_err(|_| ArcthisError::UnsupportedOperation {
                    message: "archive entry index lock was poisoned".to_owned(),
                })? = Some(entries.clone());
            return Ok(entries);
        }
        self.refresh_entries()
    }

    pub fn refresh_entries(&self) -> Result<Vec<ArchiveEntry>> {
        let mut entries = self.backend.entries()?;
        for entry in &mut entries {
            entry.mime_guess = mime_guess::from_path(&entry.path)
                .first_raw()
                .map(str::to_owned);
        }
        *self
            .entry_index
            .lock()
            .map_err(|_| ArcthisError::UnsupportedOperation {
                message: "archive entry index lock was poisoned".to_owned(),
            })? = Some(entries.clone());
        Ok(entries)
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
        self.backend.copy_entry_to(path, writer)
    }

    #[allow(clippy::too_many_lines)] // One pass keeps warning derivation consistent with the returned metadata.
    pub fn inspect(&self) -> Result<ArchiveInspection> {
        let entries = self.entries()?;
        let compressed_size = self.source.len()?;
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
                code: "encrypted_entries".to_owned(),
                message:
                    "archive content is encrypted; content operations require the correct password"
                        .to_owned(),
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
        if entries.iter().any(|entry| {
            entry.size > 0
                && entry.compressed_size.is_some_and(|compressed| {
                    compressed == 0 || entry.size > compressed.saturating_mul(1_000)
                })
        }) {
            warnings.push(ArchiveWarning {
                code: "high_compression_ratio".to_owned(),
                message: "one or more entries declare an expansion ratio above 1000:1".to_owned(),
            });
        }
        if matches!(
            self.format(),
            ArchiveFormat::Gzip | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd
        ) {
            warnings.push(ArchiveWarning {
                code: "single_stream_metadata_scan".to_owned(),
                message: "this format has no entry table; determining payload size requires a sequential decode"
                    .to_owned(),
            });
        }
        let volume_count = self.source.volume_count();
        if volume_count > 1 {
            warnings.push(ArchiveWarning {
                code: "multipart_byte_stream".to_owned(),
                message: format!(
                    "archive source combines {volume_count} explicitly ordered byte-stream volumes"
                ),
            });
        }
        if self.format() == ArchiveFormat::Rar {
            warnings.push(ArchiveWarning {
                code: "rar_metadata_limited".to_owned(),
                message: "RAR solid, encryption, and compressed-size metadata may be unavailable from the current backend"
                    .to_owned(),
            });
        }

        let compression_ratio =
            calculate_compression_ratio(compressed_size, uncompressed_size, size_overflow);
        let entry_count = u64::try_from(entries.len()).unwrap_or(u64::MAX);
        Ok(ArchiveInspection {
            compression: match self.format() {
                ArchiveFormat::Zip | ArchiveFormat::SevenZip | ArchiveFormat::Rar => {
                    "mixed".to_owned()
                }
                ArchiveFormat::Tar => "none".to_owned(),
                ArchiveFormat::TarGzip | ArchiveFormat::Gzip => "gzip".to_owned(),
                ArchiveFormat::TarBzip2 | ArchiveFormat::Bzip2 => "bzip2".to_owned(),
                ArchiveFormat::TarXz | ArchiveFormat::Xz => "xz".to_owned(),
                ArchiveFormat::TarZstd | ArchiveFormat::Zstd => "zstd".to_owned(),
            },
            encrypted,
            solid: capabilities.solid,
            random_access: capabilities.random_access,
            multipart: volume_count > 1,
            volume_count,
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

    pub fn plan_extract(
        &self,
        selected_entry: Option<&str>,
        options: &ExtractOptions,
    ) -> Result<ExtractPlan> {
        crate::extract::plan_extract_archive(self, selected_entry, options)
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

struct BoundedMemoryWriter {
    bytes: Vec<u8>,
    limit: u64,
    exceeded: bool,
}

impl BoundedMemoryWriter {
    fn new(capacity: usize, limit: u64) -> Result<Self> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(capacity)
            .map_err(|error| ArcthisError::ResourceLimit {
                message: format!("cannot reserve nested archive buffer: {error}"),
            })?;
        Ok(Self {
            bytes,
            limit,
            exceeded: false,
        })
    }
}

impl Write for BoundedMemoryWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next = u64::try_from(self.bytes.len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(buffer.len()).unwrap_or(u64::MAX));
        if next > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("nested archive memory limit exceeded"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
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
