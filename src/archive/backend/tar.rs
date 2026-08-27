use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;

use super::{ArchiveBackend, display_entry_path, format_unix_time};
use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractionStats, ExtractionWriter, PlannedEntry};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryKind,
    VerificationResult,
};
use crate::security::ExtractionLimits;

pub(crate) struct TarBackend {
    path: PathBuf,
    format: ArchiveFormat,
}

impl TarBackend {
    pub(crate) const fn new(path: PathBuf, format: ArchiveFormat) -> Self {
        Self { path, format }
    }

    fn reader(&self) -> Result<Box<dyn Read>> {
        let file = File::open(&self.path)
            .map_err(|error| ArcthisError::io("opening TAR archive", error))?;
        match self.format {
            ArchiveFormat::Tar => Ok(Box::new(file)),
            ArchiveFormat::TarGzip => Ok(Box::new(GzDecoder::new(file))),
            ArchiveFormat::Zip => Err(ArcthisError::UnsupportedOperation {
                message: "ZIP cannot be opened by the TAR backend".to_owned(),
            }),
        }
    }

    fn read_entries(&self) -> Result<Vec<ArchiveEntry>> {
        let reader = self.reader()?;
        let mut archive = tar::Archive::new(reader);
        let iterator = archive
            .entries()
            .map_err(|error| ArcthisError::InvalidArchive {
                message: error.to_string(),
            })?;
        let mut result = Vec::new();
        for entry in iterator {
            let entry = entry.map_err(|error| ArcthisError::CorruptedArchive {
                message: error.to_string(),
            })?;
            let header = entry.header();
            let entry_type = header.entry_type();
            let kind = if entry_type.is_file() {
                EntryKind::File
            } else if entry_type.is_dir() {
                EntryKind::Directory
            } else if entry_type.is_symlink() {
                EntryKind::Symlink
            } else if entry_type.is_hard_link() {
                EntryKind::Hardlink
            } else {
                EntryKind::Other
            };
            let link_target = entry
                .link_name_bytes()
                .map(|bytes| display_entry_path(bytes.as_ref()).0);
            let mode = header.mode().ok();
            let (path, path_encoding) = display_entry_path(entry.path_bytes().as_ref());
            result.push(ArchiveEntry {
                path,
                path_encoding,
                kind,
                size: header
                    .size()
                    .map_err(|error| ArcthisError::InvalidArchive {
                        message: error.to_string(),
                    })?,
                compressed_size: None,
                modified_time: header.mtime().ok().and_then(format_unix_time),
                encrypted: false,
                executable: mode.is_some_and(|value| value & 0o111 != 0),
                symlink_target: link_target,
                crc32: None,
            });
        }
        Ok(result)
    }
}

impl ArchiveBackend for TarBackend {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn capabilities(&self) -> ArchiveCapabilities {
        ArchiveCapabilities::tar()
    }

    fn validate(&self) -> Result<()> {
        let reader = self.reader()?;
        let mut archive = tar::Archive::new(reader);
        let mut entries = archive
            .entries()
            .map_err(|error| ArcthisError::InvalidArchive {
                message: error.to_string(),
            })?;
        if let Some(entry) = entries.next() {
            entry.map_err(|error| ArcthisError::CorruptedArchive {
                message: error.to_string(),
            })?;
        }
        Ok(())
    }

    fn entries(&self) -> Result<Vec<ArchiveEntry>> {
        self.read_entries()
    }

    fn copy_entry_to(&self, path: &str, writer: &mut dyn Write) -> Result<EntryCopyResult> {
        let reader = self.reader()?;
        let mut archive = tar::Archive::new(reader);
        let entries = archive
            .entries()
            .map_err(|error| ArcthisError::InvalidArchive {
                message: error.to_string(),
            })?;
        for entry in entries {
            let mut entry = entry.map_err(|error| ArcthisError::CorruptedArchive {
                message: error.to_string(),
            })?;
            if display_entry_path(entry.path_bytes().as_ref()).0 != path {
                continue;
            }
            let bytes_written = io::copy(&mut entry, writer)
                .map_err(|error| ArcthisError::io("streaming TAR entry", error))?;
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
        let reader = self.reader()?;
        let mut archive = tar::Archive::new(reader);
        let entries = archive
            .entries()
            .map_err(|error| ArcthisError::InvalidArchive {
                message: error.to_string(),
            })?;
        for entry in entries {
            let mut entry = entry.map_err(|error| ArcthisError::CorruptedArchive {
                message: error.to_string(),
            })?;
            let path = display_entry_path(entry.path_bytes().as_ref()).0;
            let Some(target) = targets.get(path.as_str()) else {
                continue;
            };
            match target.kind {
                EntryKind::Directory => writer.create_directory(target)?,
                EntryKind::File => writer.write_file(target, &mut entry)?,
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
        let reader = self.reader()?;
        let mut archive = tar::Archive::new(reader);
        let (entries_checked, bytes_checked) = {
            let entries = archive
                .entries()
                .map_err(|error| ArcthisError::VerificationFailed {
                    message: error.to_string(),
                })?;
            let mut entries_checked = 0_u64;
            let mut bytes_checked = 0_u64;
            for entry in entries {
                let mut entry = entry.map_err(|error| ArcthisError::VerificationFailed {
                    message: error.to_string(),
                })?;
                let bytes = io::copy(&mut entry, &mut io::sink()).map_err(|error| {
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
            (entries_checked, bytes_checked)
        };
        let mut remainder = archive.into_inner();
        io::copy(&mut remainder, &mut io::sink()).map_err(|error| {
            ArcthisError::VerificationFailed {
                message: error.to_string(),
            }
        })?;
        Ok(VerificationResult {
            verified: true,
            entries_checked,
            bytes_checked,
        })
    }
}
