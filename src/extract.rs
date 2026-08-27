use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use tempfile::{Builder, NamedTempFile};

use crate::archive::Archive;
use crate::error::{ArcthisError, Result};
use crate::lifecycle::{
    CollisionPolicy, OperationStatus, commit_staged_path, delete_source,
    ensure_executable_resolution, resolve_destination,
};
use crate::model::{ArchiveEntry, ArchiveFormat, EntryKind, EntryPathEncoding};
use crate::security::{ExtractionLimits, validate_entry_path};

const LIMIT_IO_MESSAGE: &str = "arcthis extraction resource limit exceeded";
const TIME_LIMIT_IO_MESSAGE: &str = "arcthis extraction time limit exceeded";

#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    pub output: Option<PathBuf>,
    pub base_directory: Option<PathBuf>,
    pub limits: ExtractionLimits,
    pub collision_policy: CollisionPolicy,
    pub delete_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtractResult {
    pub destination: PathBuf,
    pub entries_extracted: u64,
    pub bytes_written: u64,
    pub status: OperationStatus,
    pub source_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[allow(clippy::struct_excessive_bools)] // Independent plan facts are a stable machine contract.
pub struct ExtractPlan {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub selected_entry: Option<String>,
    pub entries_to_extract: u64,
    pub estimated_uncompressed_size: u64,
    pub collision: bool,
    pub collision_policy: CollisionPolicy,
    pub will_skip: bool,
    pub will_overwrite: bool,
    pub renamed_destination: bool,
    pub will_delete_source_after_success: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedEntry {
    pub archive_path: String,
    pub relative_path: PathBuf,
    pub kind: EntryKind,
    pub size: u64,
    pub executable: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExtractionStats {
    pub entries_written: u64,
    pub bytes_written: u64,
}

pub(crate) fn extract_archive(
    archive: &Archive,
    selected_entry: Option<&str>,
    options: &ExtractOptions,
) -> Result<ExtractResult> {
    if let Some(entry) = selected_entry {
        return extract_single(archive, entry, options);
    }
    extract_all(archive, options)
}

pub(crate) fn plan_extract_archive(
    archive: &Archive,
    selected_entry: Option<&str>,
    options: &ExtractOptions,
) -> Result<ExtractPlan> {
    let (requested, entries_to_extract, estimated_uncompressed_size) =
        if let Some(selected_entry) = selected_entry {
            let destination =
                options
                    .output
                    .clone()
                    .ok_or_else(|| ArcthisError::UnsupportedOperation {
                        message: "single-entry extraction requires `--output <file>`".to_owned(),
                    })?;
            let entry = archive.entry(selected_entry)?;
            if entry.kind != EntryKind::File {
                return Err(ArcthisError::UnsupportedOperation {
                    message: "single-entry extraction supports regular files only".to_owned(),
                });
            }
            enforce_declared_size(&entry, &options.limits)?;
            (destination, 1, entry.size)
        } else {
            let entries = archive.entries()?;
            enforce_entry_count(entries.len(), &options.limits)?;
            let validated = validate_entries(&entries, &options.limits)?;
            let (destination, _) = choose_destination(
                archive.path(),
                archive.format(),
                options.output.as_deref(),
                options.base_directory.as_deref(),
                &validated,
            )?;
            let size = entries
                .iter()
                .try_fold(0_u64, |total, entry| total.checked_add(entry.size))
                .ok_or_else(|| ArcthisError::ResourceLimit {
                    message: "declared extraction size overflows u64".to_owned(),
                })?;
            (
                destination,
                u64::try_from(entries.len()).unwrap_or(u64::MAX),
                size,
            )
        };
    let resolution = resolve_destination(&requested, options.collision_policy)?;
    let warnings = archive
        .inspect()?
        .warnings
        .into_iter()
        .map(|warning| format!("{}: {}", warning.code, warning.message))
        .collect();
    Ok(ExtractPlan {
        source: archive.path().to_path_buf(),
        destination: resolution.path,
        selected_entry: selected_entry.map(str::to_owned),
        entries_to_extract,
        estimated_uncompressed_size,
        collision: resolution.existed,
        collision_policy: options.collision_policy,
        will_skip: resolution.skip,
        will_overwrite: resolution.existed
            && options.collision_policy == CollisionPolicy::Overwrite,
        renamed_destination: resolution.renamed,
        will_delete_source_after_success: options.delete_source && !resolution.skip,
        warnings,
    })
}

fn extract_single(
    archive: &Archive,
    selected_entry: &str,
    options: &ExtractOptions,
) -> Result<ExtractResult> {
    let requested = options
        .output
        .clone()
        .ok_or_else(|| ArcthisError::UnsupportedOperation {
            message: "single-entry extraction requires `--output <file>`".to_owned(),
        })?;
    let entry = archive.entry(selected_entry)?;
    if entry.kind != EntryKind::File {
        return Err(ArcthisError::UnsupportedOperation {
            message: "single-entry extraction supports regular files only".to_owned(),
        });
    }
    enforce_declared_size(&entry, &options.limits)?;
    let resolution = resolve_destination(&requested, options.collision_policy)?;
    ensure_executable_resolution(&resolution, options.collision_policy)?;
    if resolution.skip {
        return Ok(ExtractResult {
            destination: resolution.path,
            entries_extracted: 0,
            bytes_written: 0,
            status: OperationStatus::Skipped,
            source_deleted: false,
        });
    }
    let destination = resolution.path;
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| ArcthisError::io("creating output parent directory", error))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| ArcthisError::io("creating temporary output file", error))?;
    let bytes_written = {
        let mut writer = LimitedWriter::new(
            temporary.as_file_mut(),
            options.limits.max_entry_size,
            options.limits.max_total_size,
            options.limits.max_entry_duration,
        );
        archive
            .copy_entry_to(selected_entry, &mut writer)
            .map_err(map_archive_stream_error)?;
        writer
            .flush()
            .map_err(|error| ArcthisError::io("flushing extracted entry", error))?;
        writer.bytes_written
    };
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| ArcthisError::io("syncing extracted entry", error))?;
    let (_, staged_path) = temporary
        .keep()
        .map_err(|error| ArcthisError::io("preserving staged extracted entry", error.error))?;
    commit_staged_path(&staged_path, &destination, options.collision_policy)?;
    let source_deleted = if options.delete_source {
        delete_source(archive.path())?;
        true
    } else {
        false
    };
    Ok(ExtractResult {
        destination,
        entries_extracted: 1,
        bytes_written,
        status: OperationStatus::Completed,
        source_deleted,
    })
}

fn extract_all(archive: &Archive, options: &ExtractOptions) -> Result<ExtractResult> {
    let entries = archive.entries()?;
    enforce_entry_count(entries.len(), &options.limits)?;
    let validated = validate_entries(&entries, &options.limits)?;

    let (requested, strip_root) = choose_destination(
        archive.path(),
        archive.format(),
        options.output.as_deref(),
        options.base_directory.as_deref(),
        &validated,
    )?;
    let resolution = resolve_destination(&requested, options.collision_policy)?;
    ensure_executable_resolution(&resolution, options.collision_policy)?;
    if resolution.skip {
        return Ok(ExtractResult {
            destination: resolution.path,
            entries_extracted: 0,
            bytes_written: 0,
            status: OperationStatus::Skipped,
            source_deleted: false,
        });
    }
    let destination = resolution.path;
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| ArcthisError::io("creating output parent directory", error))?;

    let plan = validated
        .into_iter()
        .filter_map(|(entry, path)| {
            let relative_path = if strip_root {
                let mut components = path.components();
                components.next()?;
                components.as_path().to_path_buf()
            } else {
                path
            };
            if relative_path.as_os_str().is_empty() && entry.kind == EntryKind::Directory {
                return None;
            }
            Some(PlannedEntry {
                archive_path: entry.path.clone(),
                relative_path,
                kind: entry.kind,
                size: entry.size,
                executable: entry.executable,
            })
        })
        .collect::<Vec<_>>();

    let staging = Builder::new()
        .prefix(".arcthis-tmp-")
        .tempdir_in(parent)
        .map_err(|error| ArcthisError::io("creating extraction staging directory", error))?;
    let stats = archive.extract_plan(&plan, staging.path(), &options.limits)?;
    if usize::try_from(stats.entries_written).ok() != Some(plan.len()) {
        return Err(ArcthisError::PartialFailure {
            message: format!(
                "planned {} entries but wrote {}",
                plan.len(),
                stats.entries_written
            ),
        });
    }
    let staging_path = staging.keep();
    if let Err(error) = commit_staged_path(&staging_path, &destination, options.collision_policy) {
        let _cleanup_result = fs::remove_dir_all(&staging_path);
        return Err(error);
    }
    let source_deleted = if options.delete_source {
        delete_source(archive.path())?;
        true
    } else {
        false
    };
    Ok(ExtractResult {
        destination,
        entries_extracted: stats.entries_written,
        bytes_written: stats.bytes_written,
        status: OperationStatus::Completed,
        source_deleted,
    })
}

pub(crate) fn validate_entries<'a>(
    entries: &'a [ArchiveEntry],
    limits: &ExtractionLimits,
) -> Result<Vec<(&'a ArchiveEntry, PathBuf)>> {
    let mut total_size = 0_u64;
    let mut validated = Vec::with_capacity(entries.len());
    let mut seen = HashMap::with_capacity(entries.len());
    let mut casefolded = HashMap::with_capacity(entries.len());
    for entry in entries {
        if entry.path_encoding != EntryPathEncoding::Utf8 {
            return Err(ArcthisError::UnsafePath {
                path: entry.path.clone(),
                reason: "non-UTF-8 entry paths cannot be materialized safely".to_owned(),
            });
        }
        if !matches!(entry.kind, EntryKind::File | EntryKind::Directory) {
            return Err(ArcthisError::UnsafePath {
                path: entry.path.clone(),
                reason: "links and special entries are not restored".to_owned(),
            });
        }
        let relative = validate_entry_path(&entry.path, entry.kind, limits)?;
        if seen.insert(relative.clone(), entry.kind).is_some() {
            return Err(ArcthisError::Collision {
                message: format!("duplicate archive entry path: {}", entry.path),
            });
        }
        let portable_path = PathBuf::from(relative.to_string_lossy().to_lowercase());
        if casefolded.insert(portable_path, entry.kind).is_some() {
            return Err(ArcthisError::Collision {
                message: format!("case-insensitive entry path collision: {}", entry.path),
            });
        }
        if entry.kind == EntryKind::File {
            enforce_declared_size(entry, limits)?;
            total_size =
                total_size
                    .checked_add(entry.size)
                    .ok_or_else(|| ArcthisError::ResourceLimit {
                        message: "declared total extraction size overflows u64".to_owned(),
                    })?;
            if total_size > limits.max_total_size {
                return Err(ArcthisError::ResourceLimit {
                    message: format!(
                        "declared total size {total_size} exceeds {} bytes",
                        limits.max_total_size
                    ),
                });
            }
        }
        validated.push((entry, relative));
    }
    reject_parent_file_conflicts(&seen)?;
    reject_parent_file_conflicts(&casefolded)?;
    Ok(validated)
}

pub(crate) fn enforce_entry_count(count: usize, limits: &ExtractionLimits) -> Result<()> {
    let count = u64::try_from(count).unwrap_or(u64::MAX);
    if count > limits.max_entries {
        return Err(ArcthisError::ResourceLimit {
            message: format!(
                "archive has {count} entries, above the {} entry limit",
                limits.max_entries
            ),
        });
    }
    Ok(())
}

fn enforce_declared_size(entry: &ArchiveEntry, limits: &ExtractionLimits) -> Result<()> {
    if entry.size > limits.max_entry_size {
        return Err(ArcthisError::ResourceLimit {
            message: format!(
                "entry declares {} bytes, above the {} byte limit",
                entry.size, limits.max_entry_size
            ),
        });
    }
    if let (Some(max_ratio), Some(compressed_size)) =
        (limits.max_compression_ratio, entry.compressed_size)
        && entry.size > 0
        && (compressed_size == 0 || entry.size > compressed_size.saturating_mul(max_ratio))
    {
        return Err(ArcthisError::ResourceLimit {
            message: format!(
                "entry `{}` exceeds the configured {max_ratio}:1 compression ratio limit",
                entry.path
            ),
        });
    }
    Ok(())
}

fn reject_parent_file_conflicts(paths: &HashMap<PathBuf, EntryKind>) -> Result<()> {
    for path in paths.keys() {
        let mut ancestor = path.parent();
        while let Some(parent) = ancestor {
            if parent.as_os_str().is_empty() {
                break;
            }
            if paths
                .get(parent)
                .is_some_and(|kind| *kind != EntryKind::Directory)
            {
                return Err(ArcthisError::Collision {
                    message: format!(
                        "entry path `{}` has a non-directory parent `{}`",
                        path.display(),
                        parent.display()
                    ),
                });
            }
            ancestor = parent.parent();
        }
    }
    Ok(())
}

fn choose_destination(
    archive_path: &Path,
    format: ArchiveFormat,
    explicit: Option<&Path>,
    base_directory: Option<&Path>,
    entries: &[(&crate::ArchiveEntry, PathBuf)],
) -> Result<(PathBuf, bool)> {
    if let Some(output) = explicit {
        return Ok((output.to_path_buf(), false));
    }
    let current = if let Some(base) = base_directory {
        base.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| ArcthisError::io("reading current directory", error))?
    };
    if let Some(root) = unique_top_level_directory(entries) {
        return Ok((current.join(root), true));
    }
    Ok((current.join(archive_stem(archive_path, format)?), false))
}

fn unique_top_level_directory(entries: &[(&crate::ArchiveEntry, PathBuf)]) -> Option<PathBuf> {
    let first = entries
        .first()?
        .1
        .components()
        .next()?
        .as_os_str()
        .to_owned();
    let all_same = entries.iter().all(|(_, path)| {
        path.components()
            .next()
            .is_some_and(|component| component.as_os_str() == first)
    });
    if !all_same {
        return None;
    }
    let directory_like = entries.iter().any(|(entry, path)| {
        (path.components().count() == 1 && entry.kind == EntryKind::Directory)
            || path.components().count() > 1
    });
    directory_like.then(|| PathBuf::from(first))
}

fn archive_stem(path: &Path, format: ArchiveFormat) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ArcthisError::UnsupportedOperation {
            message: "automatic extraction requires a UTF-8 archive filename".to_owned(),
        })?;
    let lower = name.to_ascii_lowercase();
    let suffixes: &[&str] = match format {
        ArchiveFormat::Zip => &[".zip"],
        ArchiveFormat::SevenZip => &[".7z"],
        ArchiveFormat::Rar => &[".rar"],
        ArchiveFormat::Tar => &[".tar"],
        ArchiveFormat::TarGzip => &[".tar.gz", ".tgz"],
        ArchiveFormat::TarBzip2 => &[".tar.bz2", ".tbz2"],
        ArchiveFormat::TarXz => &[".tar.xz", ".txz"],
        ArchiveFormat::TarZstd => &[".tar.zst", ".tzst"],
        ArchiveFormat::Gzip => &[".gz"],
        ArchiveFormat::Bzip2 => &[".bz2"],
        ArchiveFormat::Xz => &[".xz"],
        ArchiveFormat::Zstd => &[".zst"],
    };
    for suffix in suffixes {
        if lower.ends_with(suffix) && name.len() > suffix.len() {
            return Ok(name[..name.len() - suffix.len()].to_owned());
        }
    }
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| ArcthisError::UnsupportedOperation {
            message: "cannot derive an automatic extraction destination".to_owned(),
        })
}

pub(crate) struct ExtractionWriter<'a> {
    root: &'a Path,
    limits: &'a ExtractionLimits,
    stats: ExtractionStats,
    written_paths: HashSet<PathBuf>,
}

impl<'a> ExtractionWriter<'a> {
    pub(crate) fn new(root: &'a Path, limits: &'a ExtractionLimits) -> Self {
        Self {
            root,
            limits,
            stats: ExtractionStats::default(),
            written_paths: HashSet::new(),
        }
    }

    pub(crate) fn create_directory(&mut self, entry: &PlannedEntry) -> Result<()> {
        self.claim_path(&entry.relative_path)?;
        fs::create_dir_all(self.root.join(&entry.relative_path))
            .map_err(|error| ArcthisError::io("creating extracted directory", error))?;
        self.stats.entries_written += 1;
        Ok(())
    }

    pub(crate) fn write_file(&mut self, entry: &PlannedEntry, reader: &mut dyn Read) -> Result<()> {
        self.write_file_with(entry, |writer| {
            std::io::copy(reader, writer).map_err(map_extraction_io_error)
        })
    }

    pub(crate) fn write_file_with(
        &mut self,
        entry: &PlannedEntry,
        write_content: impl FnOnce(&mut dyn Write) -> Result<u64>,
    ) -> Result<()> {
        self.claim_path(&entry.relative_path)?;
        let destination = self.root.join(&entry.relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| ArcthisError::io("creating extracted file parent", error))?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| ArcthisError::io("creating extracted file", error))?;
        let remaining_total = self
            .limits
            .max_total_size
            .saturating_sub(self.stats.bytes_written);
        let written = {
            let mut writer = LimitedWriter::new(
                &mut file,
                self.limits.max_entry_size,
                remaining_total,
                self.limits.max_entry_duration,
            );
            write_content(&mut writer)?;
            writer
                .flush()
                .map_err(|error| ArcthisError::io("flushing extracted file", error))?;
            writer.bytes_written
        };
        if written != entry.size {
            return Err(ArcthisError::CorruptedArchive {
                message: format!(
                    "entry `{}` declared {} bytes but produced {written}",
                    entry.archive_path, entry.size
                ),
            });
        }
        set_executable(&file, entry.executable)?;
        file.sync_all()
            .map_err(|error| ArcthisError::io("syncing extracted file", error))?;
        self.stats.bytes_written += written;
        self.stats.entries_written += 1;
        Ok(())
    }

    fn claim_path(&mut self, path: &Path) -> Result<()> {
        if !self.written_paths.insert(path.to_path_buf()) {
            return Err(ArcthisError::Collision {
                message: format!("entry path written more than once: {}", path.display()),
            });
        }
        Ok(())
    }

    pub(crate) const fn stats(&self) -> ExtractionStats {
        self.stats
    }
}

struct LimitedWriter<W> {
    inner: W,
    max_entry: u64,
    max_total: u64,
    bytes_written: u64,
    started: Instant,
    max_duration: Option<std::time::Duration>,
}

impl<W> LimitedWriter<W> {
    fn new(
        inner: W,
        max_entry: u64,
        max_total: u64,
        max_duration: Option<std::time::Duration>,
    ) -> Self {
        Self {
            inner,
            max_entry,
            max_total,
            bytes_written: 0,
            started: Instant::now(),
            max_duration,
        }
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self
            .max_duration
            .is_some_and(|duration| self.started.elapsed() > duration)
        {
            return Err(std::io::Error::other(TIME_LIMIT_IO_MESSAGE));
        }
        let requested = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        if self.bytes_written.saturating_add(requested) > self.max_entry
            || self.bytes_written.saturating_add(requested) > self.max_total
        {
            return Err(std::io::Error::other(LIMIT_IO_MESSAGE));
        }
        let written = self.inner.write(buffer)?;
        self.bytes_written += u64::try_from(written).unwrap_or(u64::MAX);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn map_extraction_io_error(error: io::Error) -> ArcthisError {
    if matches!(
        error.to_string().as_str(),
        LIMIT_IO_MESSAGE | TIME_LIMIT_IO_MESSAGE
    ) {
        ArcthisError::ResourceLimit {
            message: error.to_string(),
        }
    } else {
        ArcthisError::io("streaming extracted file", error)
    }
}

fn map_archive_stream_error(error: ArcthisError) -> ArcthisError {
    let limit_reached = matches!(
        &error,
        ArcthisError::Io { source, .. } | ArcthisError::PermissionDenied { source, .. }
            if matches!(source.to_string().as_str(), LIMIT_IO_MESSAGE | TIME_LIMIT_IO_MESSAGE)
    );
    if limit_reached {
        ArcthisError::ResourceLimit {
            message: "actual extraction exceeded the configured resource limit".to_owned(),
        }
    } else {
        error
    }
}

#[cfg(unix)]
fn set_executable(file: &File, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if executable { 0o755 } else { 0o644 };
    file.set_permissions(fs::Permissions::from_mode(mode))
        .map_err(|error| ArcthisError::io("setting extracted file permissions", error))
}

#[cfg(not(unix))]
fn set_executable(_file: &File, _executable: bool) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{archive_stem, reject_parent_file_conflicts};
    use crate::model::{ArchiveFormat, EntryKind};

    #[test]
    fn compound_archive_stem_is_removed_as_a_unit() {
        assert_eq!(
            archive_stem(Path::new("backup.tar.gz"), ArchiveFormat::TarGzip).expect("archive stem"),
            "backup"
        );
    }

    #[test]
    fn rejects_file_as_parent_of_another_entry() {
        let paths = [
            (PathBuf::from("parent"), EntryKind::File),
            (PathBuf::from("parent/child"), EntryKind::File),
        ]
        .into_iter()
        .collect();
        assert!(reject_parent_file_conflicts(&paths).is_err());
    }
}
