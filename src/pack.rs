use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use bzip2::Compression as Bzip2Compression;
use bzip2::write::BzEncoder;
use flate2::Compression;
use flate2::write::GzEncoder;
use lzma_rust2::{XzOptions, XzWriter};
use serde::Serialize;
use tempfile::NamedTempFile;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::archive::Archive;
use crate::error::{ArcthisError, Result};
use crate::lifecycle::{
    CollisionPolicy, OperationStatus, commit_staged_path, delete_source,
    ensure_destination_outside_source, ensure_executable_resolution, resolve_destination,
};
use crate::model::{ArchiveFormat, VerificationResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackResult {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub format: ArchiveFormat,
    pub entries_packed: u64,
    pub archive_size: u64,
    pub verification: VerificationResult,
    pub status: OperationStatus,
    pub source_deleted: bool,
}

#[derive(Debug, Clone)]
pub struct PackOptions {
    pub collision_policy: CollisionPolicy,
    pub delete_source: bool,
    /// Include the source basename as the first archive path component.
    pub include_source_root: bool,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            collision_policy: CollisionPolicy::Refuse,
            delete_source: false,
            include_source_root: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[allow(clippy::struct_excessive_bools)] // Independent plan facts are a stable machine contract.
pub struct PackPlan {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub format: ArchiveFormat,
    pub entries_to_pack: u64,
    pub estimated_input_size: u64,
    pub collision: bool,
    pub collision_policy: CollisionPolicy,
    pub will_skip: bool,
    pub will_overwrite: bool,
    pub renamed_destination: bool,
    pub will_delete_source_after_success: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    File,
    Directory,
}

#[derive(Debug)]
struct SourceEntry {
    source_path: PathBuf,
    archive_path: PathBuf,
    kind: SourceKind,
    unix_mode: u32,
}

pub fn pack_source(source: &Path, output: &Path) -> Result<PackResult> {
    pack_source_with_options(source, output, &PackOptions::default())
}

pub fn plan_pack_source(source: &Path, output: &Path, options: &PackOptions) -> Result<PackPlan> {
    let format = output_format(output)?;
    let source = fs::canonicalize(source)
        .map_err(|error| ArcthisError::io("resolving pack source", error))?;
    let resolution = resolve_destination(output, options.collision_policy)?;
    ensure_destination_outside_source(&source, &resolution.path)?;
    let source_entries = collect_source_entries(&source, options.include_source_root)?;
    let estimated_input_size = source_entries.iter().try_fold(0_u64, |total, entry| {
        let size = if entry.kind == SourceKind::File {
            fs::metadata(&entry.source_path)
                .map_err(|error| ArcthisError::io("reading pack source size", error))?
                .len()
        } else {
            0
        };
        total
            .checked_add(size)
            .ok_or_else(|| ArcthisError::ResourceLimit {
                message: "pack input size overflows u64".to_owned(),
            })
    })?;
    Ok(PackPlan {
        source,
        destination: resolution.path,
        format,
        entries_to_pack: u64::try_from(source_entries.len()).unwrap_or(u64::MAX),
        estimated_input_size,
        collision: resolution.existed,
        collision_policy: options.collision_policy,
        will_skip: resolution.skip,
        will_overwrite: resolution.existed
            && options.collision_policy == CollisionPolicy::Overwrite,
        renamed_destination: resolution.renamed,
        will_delete_source_after_success: options.delete_source && !resolution.skip,
    })
}

pub fn pack_source_with_options(
    source: &Path,
    output: &Path,
    options: &PackOptions,
) -> Result<PackResult> {
    let format = output_format(output)?;
    let source = fs::canonicalize(source)
        .map_err(|error| ArcthisError::io("resolving pack source", error))?;
    let resolution = resolve_destination(output, options.collision_policy)?;
    ensure_destination_outside_source(&source, &resolution.path)?;
    let source_entries = collect_source_entries(&source, options.include_source_root)?;
    ensure_executable_resolution(&resolution, options.collision_policy)?;
    if resolution.skip {
        let existing = Archive::open(resolution.path.as_path())?;
        let verification = existing.verify()?;
        let archive_size = fs::metadata(&resolution.path)
            .map_err(|error| ArcthisError::io("reading existing archive metadata", error))?
            .len();
        return Ok(PackResult {
            source,
            destination: resolution.path,
            format: existing.format(),
            entries_packed: 0,
            archive_size,
            verification,
            status: OperationStatus::Skipped,
            source_deleted: false,
        });
    }
    let output = resolution.path;
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| ArcthisError::io("creating archive output parent", error))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| ArcthisError::io("creating temporary archive output", error))?;

    match format {
        ArchiveFormat::Zip => write_zip(temporary.as_file_mut(), &source_entries)?,
        ArchiveFormat::SevenZip => write_seven_zip(temporary.as_file_mut(), &source_entries)?,
        ArchiveFormat::Tar => write_tar(temporary.as_file_mut(), &source_entries)?,
        ArchiveFormat::TarGzip => write_tar_gzip(temporary.as_file_mut(), &source_entries)?,
        ArchiveFormat::TarBzip2 => write_tar_bzip2(temporary.as_file_mut(), &source_entries)?,
        ArchiveFormat::TarXz => write_tar_xz(temporary.as_file_mut(), &source_entries)?,
        ArchiveFormat::TarZstd => write_tar_zstd(temporary.as_file_mut(), &source_entries)?,
        ArchiveFormat::Gzip => write_single_stream(
            temporary.as_file_mut(),
            &source_entries,
            ArchiveFormat::Gzip,
        )?,
        ArchiveFormat::Bzip2 => write_single_stream(
            temporary.as_file_mut(),
            &source_entries,
            ArchiveFormat::Bzip2,
        )?,
        ArchiveFormat::Xz => {
            write_single_stream(temporary.as_file_mut(), &source_entries, ArchiveFormat::Xz)?;
        }
        ArchiveFormat::Zstd => write_single_stream(
            temporary.as_file_mut(),
            &source_entries,
            ArchiveFormat::Zstd,
        )?,
        ArchiveFormat::Rar => {
            return Err(ArcthisError::UnsupportedOperation {
                message: "RAR creation is intentionally unsupported".to_owned(),
            });
        }
    }
    temporary
        .as_file_mut()
        .flush()
        .map_err(|error| ArcthisError::io("flushing archive output", error))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| ArcthisError::io("syncing archive output", error))?;

    let verification = {
        let archive = Archive::open(temporary.path())?;
        archive.verify()?
    };
    let archive_size = temporary
        .as_file()
        .metadata()
        .map_err(|error| ArcthisError::io("reading packed archive metadata", error))?
        .len();
    let (_, staged_path) = temporary
        .keep()
        .map_err(|error| ArcthisError::io("preserving staged archive", error.error))?;
    commit_staged_path(&staged_path, &output, options.collision_policy)?;
    let source_deleted = if options.delete_source {
        delete_source(&source)?;
        true
    } else {
        false
    };
    Ok(PackResult {
        source,
        destination: output,
        format,
        entries_packed: u64::try_from(source_entries.len()).unwrap_or(u64::MAX),
        archive_size,
        verification,
        status: OperationStatus::Completed,
        source_deleted,
    })
}

pub(crate) fn output_format(output: &Path) -> Result<ArchiveFormat> {
    let extension = output.extension().and_then(|value| value.to_str());
    let inner_extension = output
        .file_stem()
        .and_then(|stem| Path::new(stem).extension())
        .and_then(|value| value.to_str());
    let is_compound = |short: &str, long: &str| {
        extension.is_some_and(|value| value.eq_ignore_ascii_case(short))
            || (extension.is_some_and(|value| value.eq_ignore_ascii_case(long))
                && inner_extension.is_some_and(|value| value.eq_ignore_ascii_case("tar")))
    };
    if is_compound("tgz", "gz") {
        Ok(ArchiveFormat::TarGzip)
    } else if is_compound("tbz2", "bz2") {
        Ok(ArchiveFormat::TarBzip2)
    } else if is_compound("txz", "xz") {
        Ok(ArchiveFormat::TarXz)
    } else if is_compound("tzst", "zst") {
        Ok(ArchiveFormat::TarZstd)
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("tar")) {
        Ok(ArchiveFormat::Tar)
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("zip")) {
        Ok(ArchiveFormat::Zip)
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("7z")) {
        Ok(ArchiveFormat::SevenZip)
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("gz")) {
        Ok(ArchiveFormat::Gzip)
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("bz2")) {
        Ok(ArchiveFormat::Bzip2)
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("xz")) {
        Ok(ArchiveFormat::Xz)
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("zst")) {
        Ok(ArchiveFormat::Zstd)
    } else {
        Err(ArcthisError::UnsupportedFormat {
            path: output.to_path_buf(),
        })
    }
}

fn collect_source_entries(source: &Path, include_source_root: bool) -> Result<Vec<SourceEntry>> {
    let original_metadata = fs::symlink_metadata(source)
        .map_err(|error| ArcthisError::io("reading pack source metadata", error))?;
    if original_metadata.file_type().is_symlink() {
        return Err(ArcthisError::UnsupportedOperation {
            message: "packing a symlink source is not supported in v0.1".to_owned(),
        });
    }
    let source = fs::canonicalize(source)
        .map_err(|error| ArcthisError::io("resolving pack source", error))?;
    let base = if include_source_root {
        source
            .parent()
            .ok_or_else(|| ArcthisError::UnsupportedOperation {
                message: "packing a filesystem root is not supported".to_owned(),
            })?
    } else {
        source.as_path()
    };

    let mut result = Vec::new();
    for entry in WalkDir::new(&source)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(map_walk_error)?;
        if !include_source_root && entry.depth() == 0 {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| ArcthisError::io("reading pack entry metadata", error))?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(ArcthisError::UnsupportedOperation {
                message: format!(
                    "packing symlinks is not supported in v0.1: {}",
                    entry.path().display()
                ),
            });
        }
        let kind = if file_type.is_dir() {
            SourceKind::Directory
        } else if file_type.is_file() {
            SourceKind::File
        } else {
            return Err(ArcthisError::UnsupportedOperation {
                message: format!(
                    "packing special files is not supported: {}",
                    entry.path().display()
                ),
            });
        };
        let archive_path = entry
            .path()
            .strip_prefix(base)
            .map_err(|error| ArcthisError::UnsupportedOperation {
                message: format!("cannot derive archive path: {error}"),
            })?
            .to_path_buf();
        result.push(SourceEntry {
            source_path: entry.path().to_path_buf(),
            archive_path,
            kind,
            unix_mode: unix_mode(&metadata, kind),
        });
    }
    Ok(result)
}

fn map_walk_error(error: walkdir::Error) -> ArcthisError {
    let kind = error
        .io_error()
        .map_or(io::ErrorKind::Other, io::Error::kind);
    ArcthisError::io("scanning pack source", io::Error::new(kind, error))
}

fn write_zip(file: &mut File, entries: &[SourceEntry]) -> Result<()> {
    let mut archive = ZipWriter::new(file);
    for entry in entries {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(entry.unix_mode);
        match entry.kind {
            SourceKind::Directory => archive
                .add_directory_from_path(&entry.archive_path, options)
                .map_err(map_zip_write_error)?,
            SourceKind::File => {
                archive
                    .start_file_from_path(&entry.archive_path, options)
                    .map_err(map_zip_write_error)?;
                let mut source = File::open(&entry.source_path)
                    .map_err(|error| ArcthisError::io("opening pack source file", error))?;
                io::copy(&mut source, &mut archive)
                    .map_err(|error| ArcthisError::io("writing ZIP entry", error))?;
            }
        }
    }
    archive.finish().map_err(map_zip_write_error)?;
    Ok(())
}

fn write_tar(file: &mut File, entries: &[SourceEntry]) -> Result<()> {
    let mut archive = tar::Builder::new(file);
    append_tar_entries(&mut archive, entries)?;
    archive
        .finish()
        .map_err(|error| ArcthisError::io("finalizing TAR archive", error))
}

fn write_tar_gzip(file: &mut File, entries: &[SourceEntry]) -> Result<()> {
    let gzip = GzEncoder::new(file, Compression::default());
    let mut archive = tar::Builder::new(gzip);
    append_tar_entries(&mut archive, entries)?;
    let gzip = archive
        .into_inner()
        .map_err(|error| ArcthisError::io("finalizing TAR archive", error))?;
    gzip.finish()
        .map_err(|error| ArcthisError::io("finalizing Gzip stream", error))?;
    Ok(())
}

fn write_tar_bzip2(file: &mut File, entries: &[SourceEntry]) -> Result<()> {
    let bzip2 = BzEncoder::new(file, Bzip2Compression::default());
    let mut archive = tar::Builder::new(bzip2);
    append_tar_entries(&mut archive, entries)?;
    let bzip2 = archive
        .into_inner()
        .map_err(|error| ArcthisError::io("finalizing TAR archive", error))?;
    bzip2
        .finish()
        .map_err(|error| ArcthisError::io("finalizing Bzip2 stream", error))?;
    Ok(())
}

fn write_tar_xz(file: &mut File, entries: &[SourceEntry]) -> Result<()> {
    let xz = XzWriter::new(file, XzOptions::default())
        .map_err(|error| ArcthisError::io("creating XZ stream", error))?;
    let mut archive = tar::Builder::new(xz);
    append_tar_entries(&mut archive, entries)?;
    let xz = archive
        .into_inner()
        .map_err(|error| ArcthisError::io("finalizing TAR archive", error))?;
    xz.finish()
        .map_err(|error| ArcthisError::io("finalizing XZ stream", error))?;
    Ok(())
}

fn write_tar_zstd(file: &mut File, entries: &[SourceEntry]) -> Result<()> {
    let zstd = zstd::stream::write::Encoder::new(file, 3)
        .map_err(|error| ArcthisError::io("creating Zstandard stream", error))?;
    let mut archive = tar::Builder::new(zstd);
    append_tar_entries(&mut archive, entries)?;
    let zstd = archive
        .into_inner()
        .map_err(|error| ArcthisError::io("finalizing TAR archive", error))?;
    zstd.finish()
        .map_err(|error| ArcthisError::io("finalizing Zstandard stream", error))?;
    Ok(())
}

fn write_seven_zip(file: &mut File, entries: &[SourceEntry]) -> Result<()> {
    let mut writer = sevenz_rust2::ArchiveWriter::new(file).map_err(map_seven_zip_write_error)?;
    for entry in entries {
        let archive_path =
            entry
                .archive_path
                .to_str()
                .ok_or_else(|| ArcthisError::UnsupportedOperation {
                    message: format!(
                        "7z creation requires UTF-8 archive paths: {}",
                        entry.archive_path.display()
                    ),
                })?;
        let archive_entry = sevenz_rust2::ArchiveEntry::from_path(
            &entry.source_path,
            archive_path.replace(std::path::MAIN_SEPARATOR, "/"),
        );
        match entry.kind {
            SourceKind::Directory => {
                writer
                    .push_archive_entry::<File>(archive_entry, None)
                    .map_err(map_seven_zip_write_error)?;
            }
            SourceKind::File => {
                let source = File::open(&entry.source_path)
                    .map_err(|error| ArcthisError::io("opening 7z source file", error))?;
                writer
                    .push_archive_entry(archive_entry, Some(source))
                    .map_err(map_seven_zip_write_error)?;
            }
        }
    }
    writer
        .finish()
        .map_err(|error| ArcthisError::io("finalizing 7z archive", error))?;
    Ok(())
}

fn write_single_stream(
    file: &mut File,
    entries: &[SourceEntry],
    format: ArchiveFormat,
) -> Result<()> {
    let [entry] = entries else {
        return Err(ArcthisError::UnsupportedOperation {
            message: format!("{format} output requires one regular source file"),
        });
    };
    if entry.kind != SourceKind::File {
        return Err(ArcthisError::UnsupportedOperation {
            message: format!("{format} output requires one regular source file"),
        });
    }
    let mut source = File::open(&entry.source_path)
        .map_err(|error| ArcthisError::io("opening single-stream source", error))?;
    match format {
        ArchiveFormat::Gzip => {
            let mut encoder = GzEncoder::new(file, Compression::default());
            io::copy(&mut source, &mut encoder)
                .map_err(|error| ArcthisError::io("writing Gzip stream", error))?;
            encoder
                .finish()
                .map_err(|error| ArcthisError::io("finalizing Gzip stream", error))?;
        }
        ArchiveFormat::Bzip2 => {
            let mut encoder = BzEncoder::new(file, Bzip2Compression::default());
            io::copy(&mut source, &mut encoder)
                .map_err(|error| ArcthisError::io("writing Bzip2 stream", error))?;
            encoder
                .finish()
                .map_err(|error| ArcthisError::io("finalizing Bzip2 stream", error))?;
        }
        ArchiveFormat::Xz => {
            let mut encoder = XzWriter::new(file, XzOptions::default())
                .map_err(|error| ArcthisError::io("creating XZ stream", error))?;
            io::copy(&mut source, &mut encoder)
                .map_err(|error| ArcthisError::io("writing XZ stream", error))?;
            encoder
                .finish()
                .map_err(|error| ArcthisError::io("finalizing XZ stream", error))?;
        }
        ArchiveFormat::Zstd => {
            let mut encoder = zstd::stream::write::Encoder::new(file, 3)
                .map_err(|error| ArcthisError::io("creating Zstandard stream", error))?;
            io::copy(&mut source, &mut encoder)
                .map_err(|error| ArcthisError::io("writing Zstandard stream", error))?;
            encoder
                .finish()
                .map_err(|error| ArcthisError::io("finalizing Zstandard stream", error))?;
        }
        _ => {
            return Err(ArcthisError::UnsupportedOperation {
                message: format!("{format} is not a single-stream format"),
            });
        }
    }
    Ok(())
}

fn map_seven_zip_write_error(error: sevenz_rust2::Error) -> ArcthisError {
    match error {
        sevenz_rust2::Error::Io(source, _) | sevenz_rust2::Error::FileOpen(source, _) => {
            ArcthisError::io("writing 7z archive", source)
        }
        sevenz_rust2::Error::UnsupportedCompressionMethod(message) => {
            ArcthisError::UnsupportedOperation { message }
        }
        sevenz_rust2::Error::Unsupported(message) => ArcthisError::UnsupportedOperation {
            message: message.into_owned(),
        },
        other => ArcthisError::InvalidArchive {
            message: other.to_string(),
        },
    }
}

fn append_tar_entries<W: Write>(
    archive: &mut tar::Builder<W>,
    entries: &[SourceEntry],
) -> Result<()> {
    for entry in entries {
        match entry.kind {
            SourceKind::Directory => archive
                .append_dir(&entry.archive_path, &entry.source_path)
                .map_err(|error| ArcthisError::io("writing TAR directory", error))?,
            SourceKind::File => archive
                .append_path_with_name(&entry.source_path, &entry.archive_path)
                .map_err(|error| ArcthisError::io("writing TAR file", error))?,
        }
    }
    Ok(())
}

fn map_zip_write_error(error: zip::result::ZipError) -> ArcthisError {
    match error {
        zip::result::ZipError::Io(source) => ArcthisError::io("writing ZIP archive", source),
        zip::result::ZipError::UnsupportedArchive(message) => ArcthisError::UnsupportedOperation {
            message: message.to_owned(),
        },
        other => ArcthisError::InvalidArchive {
            message: other.to_string(),
        },
    }
}

#[cfg(unix)]
fn unix_mode(metadata: &fs::Metadata, _kind: SourceKind) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode()
}

#[cfg(not(unix))]
fn unix_mode(_metadata: &fs::Metadata, kind: SourceKind) -> u32 {
    match kind {
        SourceKind::File => 0o644,
        SourceKind::Directory => 0o755,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::output_format;
    use crate::model::ArchiveFormat;

    #[test]
    fn compound_output_extension_selects_tar_gzip() {
        assert_eq!(
            output_format(Path::new("backup.tar.gz")).expect("output format"),
            ArchiveFormat::TarGzip
        );
    }
}
