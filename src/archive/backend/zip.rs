use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use zip::ZipArchive;
use zip::result::ZipError;

use super::{ArchiveBackend, display_entry_path};
use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractionStats, ExtractionWriter, PlannedEntry};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryKind,
    VerificationResult,
};
use crate::security::ExtractionLimits;

pub(crate) struct ZipBackend {
    path: PathBuf,
}

impl ZipBackend {
    pub(crate) const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn open(&self) -> Result<ZipArchive<File>> {
        let file = File::open(&self.path)
            .map_err(|error| ArcthisError::io("opening ZIP archive", error))?;
        ZipArchive::new(file).map_err(map_zip_error)
    }
}

impl ArchiveBackend for ZipBackend {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Zip
    }

    fn capabilities(&self) -> ArchiveCapabilities {
        ArchiveCapabilities::zip()
    }

    fn validate(&self) -> Result<()> {
        self.open().map(|_| ())
    }

    fn entries(&self) -> Result<Vec<ArchiveEntry>> {
        let mut archive = self.open()?;
        let mut entries = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let file = archive.by_index_raw(index).map_err(map_zip_error)?;
            let (path, path_encoding) = display_entry_path(file.name_raw());
            let mode = file.unix_mode();
            let is_directory = file.is_dir();
            let kind = match mode.map(|value| value & 0o170_000) {
                Some(0o120_000) => EntryKind::Symlink,
                _ if is_directory => EntryKind::Directory,
                _ => EntryKind::File,
            };
            entries.push(ArchiveEntry {
                path,
                path_encoding,
                kind,
                size: file.size(),
                compressed_size: Some(file.compressed_size()),
                modified_time: file
                    .last_modified()
                    .map(|value| value.to_string().replace(' ', "T")),
                encrypted: file.encrypted(),
                executable: mode.is_some_and(|value| value & 0o111 != 0),
                symlink_target: None,
                crc32: Some(format!("{:08x}", file.crc32())),
            });
        }
        Ok(entries)
    }

    fn copy_entry_to(&self, path: &str, writer: &mut dyn Write) -> Result<EntryCopyResult> {
        let mut archive = self.open()?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).map_err(map_zip_error)?;
            if display_entry_path(file.name_raw()).0 != path {
                continue;
            }
            if file.encrypted() {
                return Err(ArcthisError::PasswordRequired);
            }
            let bytes_written = io::copy(&mut file, writer).map_err(|error| {
                if error.kind() == io::ErrorKind::InvalidData {
                    ArcthisError::CorruptedArchive {
                        message: error.to_string(),
                    }
                } else {
                    ArcthisError::io("streaming ZIP entry", error)
                }
            })?;
            return Ok(EntryCopyResult { bytes_written });
        }
        Err(ArcthisError::EntryNotFound {
            entry: path.to_owned(),
        })
    }

    fn extract_plan(
        &self,
        plan: &[PlannedEntry],
        staging_root: &Path,
        limits: &ExtractionLimits,
    ) -> Result<ExtractionStats> {
        let targets = plan
            .iter()
            .map(|entry| (entry.archive_path.as_str(), entry))
            .collect::<std::collections::HashMap<_, _>>();
        let mut writer = ExtractionWriter::new(staging_root, limits);
        let mut archive = self.open()?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).map_err(map_zip_error)?;
            let path = display_entry_path(file.name_raw()).0;
            let Some(target) = targets.get(path.as_str()) else {
                continue;
            };
            match target.kind {
                EntryKind::Directory => writer.create_directory(target)?,
                EntryKind::File => writer.write_file(target, &mut file)?,
                EntryKind::Symlink | EntryKind::Hardlink | EntryKind::Other => {
                    return Err(ArcthisError::UnsafePath {
                        path,
                        reason: "links and special entries cannot be extracted".to_owned(),
                    });
                }
            }
        }
        Ok(writer.stats())
    }

    fn verify(&self) -> Result<VerificationResult> {
        let mut archive = self.open()?;
        let mut entries_checked = 0_u64;
        let mut bytes_checked = 0_u64;
        for index in 0..archive.len() {
            if archive
                .by_index_raw(index)
                .map_err(map_zip_error)?
                .encrypted()
            {
                return Err(ArcthisError::PasswordRequired);
            }
            let mut file = archive.by_index(index).map_err(map_zip_error)?;
            let bytes = io::copy(&mut file, &mut io::sink()).map_err(|error| {
                ArcthisError::VerificationFailed {
                    message: error.to_string(),
                }
            })?;
            bytes_checked = bytes_checked.checked_add(bytes).ok_or_else(|| {
                ArcthisError::VerificationFailed {
                    message: "verified byte count overflowed u64".to_owned(),
                }
            })?;
            entries_checked += 1;
        }
        Ok(VerificationResult {
            verified: true,
            entries_checked,
            bytes_checked,
        })
    }
}

pub(crate) fn map_zip_error(error: ZipError) -> ArcthisError {
    match error {
        ZipError::Io(source) => ArcthisError::io("reading ZIP archive", source),
        ZipError::InvalidArchive(message) => ArcthisError::InvalidArchive {
            message: message.into_owned(),
        },
        ZipError::UnsupportedArchive(message) => ArcthisError::UnsupportedOperation {
            message: message.to_owned(),
        },
        other => ArcthisError::InvalidArchive {
            message: other.to_string(),
        },
    }
}
