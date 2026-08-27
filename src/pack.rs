use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use serde::Serialize;
use tempfile::NamedTempFile;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::archive::Archive;
use crate::error::{ArcthisError, Result};
use crate::model::{ArchiveFormat, VerificationResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackResult {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub format: ArchiveFormat,
    pub entries_packed: u64,
    pub archive_size: u64,
    pub verification: VerificationResult,
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
    let format = output_format(output)?;
    prepare_output(output)?;
    let source_entries = collect_source_entries(source)?;
    let source = fs::canonicalize(source)
        .map_err(|error| ArcthisError::io("resolving pack source", error))?;
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| ArcthisError::io("creating archive output parent", error))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| ArcthisError::io("creating temporary archive output", error))?;

    match format {
        ArchiveFormat::Zip => write_zip(temporary.as_file_mut(), &source_entries)?,
        ArchiveFormat::Tar => write_tar(temporary.as_file_mut(), &source_entries)?,
        ArchiveFormat::TarGzip => write_tar_gzip(temporary.as_file_mut(), &source_entries)?,
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
    temporary.persist_noclobber(output).map_err(|error| {
        if error.error.kind() == io::ErrorKind::AlreadyExists {
            ArcthisError::Collision {
                message: format!("archive output already exists: {}", output.display()),
            }
        } else {
            ArcthisError::io("committing packed archive", error.error)
        }
    })?;
    Ok(PackResult {
        source,
        destination: output.to_path_buf(),
        format,
        entries_packed: u64::try_from(source_entries.len()).unwrap_or(u64::MAX),
        archive_size,
        verification,
    })
}

fn output_format(output: &Path) -> Result<ArchiveFormat> {
    let extension = output.extension().and_then(|value| value.to_str());
    let is_tar_gzip = extension.is_some_and(|value| value.eq_ignore_ascii_case("tgz"))
        || (extension.is_some_and(|value| value.eq_ignore_ascii_case("gz"))
            && output
                .file_stem()
                .and_then(|stem| Path::new(stem).extension())
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("tar")));
    if is_tar_gzip {
        Ok(ArchiveFormat::TarGzip)
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("tar")) {
        Ok(ArchiveFormat::Tar)
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("zip")) {
        Ok(ArchiveFormat::Zip)
    } else {
        Err(ArcthisError::UnsupportedFormat {
            path: output.to_path_buf(),
        })
    }
}

fn prepare_output(output: &Path) -> Result<()> {
    if output
        .try_exists()
        .map_err(|error| ArcthisError::io("checking archive output", error))?
    {
        return Err(ArcthisError::Collision {
            message: format!("archive output already exists: {}", output.display()),
        });
    }
    Ok(())
}

fn collect_source_entries(source: &Path) -> Result<Vec<SourceEntry>> {
    let original_metadata = fs::symlink_metadata(source)
        .map_err(|error| ArcthisError::io("reading pack source metadata", error))?;
    if original_metadata.file_type().is_symlink() {
        return Err(ArcthisError::UnsupportedOperation {
            message: "packing a symlink source is not supported in v0.1".to_owned(),
        });
    }
    let source = fs::canonicalize(source)
        .map_err(|error| ArcthisError::io("resolving pack source", error))?;
    let parent = source
        .parent()
        .ok_or_else(|| ArcthisError::UnsupportedOperation {
            message: "packing a filesystem root is not supported".to_owned(),
        })?;

    let mut result = Vec::new();
    for entry in WalkDir::new(&source)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(map_walk_error)?;
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
            .strip_prefix(parent)
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
