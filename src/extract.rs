use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tempfile::{Builder, NamedTempFile};

use crate::archive::Archive;
use crate::error::{ArcthisError, Result};
use crate::model::{ArchiveEntry, ArchiveFormat, EntryKind, EntryPathEncoding};
use crate::security::{ExtractionLimits, validate_entry_path};

const LIMIT_IO_MESSAGE: &str = "arcthis extraction resource limit exceeded";

#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    pub output: Option<PathBuf>,
    pub limits: ExtractionLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtractResult {
    pub destination: PathBuf,
    pub entries_extracted: u64,
    pub bytes_written: u64,
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

fn extract_single(
    archive: &Archive,
    selected_entry: &str,
    options: &ExtractOptions,
) -> Result<ExtractResult> {
    let destination = options
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
    enforce_declared_size(entry.size, &options.limits)?;
    prepare_absent_destination(&destination)?;
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
    temporary.persist_noclobber(&destination).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            ArcthisError::Collision {
                message: format!("destination already exists: {}", destination.display()),
            }
        } else {
            ArcthisError::io("committing extracted entry", error.error)
        }
    })?;
    Ok(ExtractResult {
        destination,
        entries_extracted: 1,
        bytes_written,
    })
}

fn extract_all(archive: &Archive, options: &ExtractOptions) -> Result<ExtractResult> {
    let entries = archive.entries()?;
    enforce_entry_count(entries.len(), &options.limits)?;
    let validated = validate_entries(&entries, &options.limits)?;

    let (destination, strip_root) = choose_destination(
        archive.path(),
        archive.format(),
        options.output.as_deref(),
        &validated,
    )?;
    prepare_absent_destination(&destination)?;
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
    if let Err(error) = fs::rename(&staging_path, &destination) {
        let _cleanup_result = fs::remove_dir_all(&staging_path);
        return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
            ArcthisError::Collision {
                message: format!("destination already exists: {}", destination.display()),
            }
        } else {
            ArcthisError::io("committing extraction directory", error)
        });
    }
    Ok(ExtractResult {
        destination,
        entries_extracted: stats.entries_written,
        bytes_written: stats.bytes_written,
    })
}

fn validate_entries<'a>(
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
                reason: "non-UTF-8 entry paths are not materialized in v0.1".to_owned(),
            });
        }
        if !matches!(entry.kind, EntryKind::File | EntryKind::Directory) {
            return Err(ArcthisError::UnsafePath {
                path: entry.path.clone(),
                reason: "links and special entries are not restored in v0.1".to_owned(),
            });
        }
        if entry.encrypted {
            return Err(ArcthisError::PasswordRequired);
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
            enforce_declared_size(entry.size, limits)?;
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

fn enforce_entry_count(count: usize, limits: &ExtractionLimits) -> Result<()> {
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

fn enforce_declared_size(size: u64, limits: &ExtractionLimits) -> Result<()> {
    if size > limits.max_entry_size {
        return Err(ArcthisError::ResourceLimit {
            message: format!(
                "entry declares {size} bytes, above the {} byte limit",
                limits.max_entry_size
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
    entries: &[(&crate::ArchiveEntry, PathBuf)],
) -> Result<(PathBuf, bool)> {
    if let Some(output) = explicit {
        return Ok((output.to_path_buf(), false));
    }
    let current = std::env::current_dir()
        .map_err(|error| ArcthisError::io("reading current directory", error))?;
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
        ArchiveFormat::Tar => &[".tar"],
        ArchiveFormat::TarGzip => &[".tar.gz", ".tgz"],
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

fn prepare_absent_destination(destination: &Path) -> Result<()> {
    if destination
        .try_exists()
        .map_err(|error| ArcthisError::io("checking extraction destination", error))?
    {
        return Err(ArcthisError::Collision {
            message: format!("destination already exists: {}", destination.display()),
        });
    }
    Ok(())
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
            let mut writer =
                LimitedWriter::new(&mut file, self.limits.max_entry_size, remaining_total);
            std::io::copy(reader, &mut writer).map_err(map_extraction_io_error)?;
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
}

impl<W> LimitedWriter<W> {
    const fn new(inner: W, max_entry: u64, max_total: u64) -> Self {
        Self {
            inner,
            max_entry,
            max_total,
            bytes_written: 0,
        }
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
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
    if error.to_string() == LIMIT_IO_MESSAGE {
        ArcthisError::ResourceLimit {
            message: "actual extracted bytes exceeded the configured limit".to_owned(),
        }
    } else {
        ArcthisError::io("streaming extracted file", error)
    }
}

fn map_archive_stream_error(error: ArcthisError) -> ArcthisError {
    let limit_reached = matches!(
        &error,
        ArcthisError::Io { source, .. } | ArcthisError::PermissionDenied { source, .. }
            if source.to_string() == LIMIT_IO_MESSAGE
    );
    if limit_reached {
        ArcthisError::ResourceLimit {
            message: "actual extracted bytes exceeded the configured limit".to_owned(),
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
