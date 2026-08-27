use std::io::{self, Write};
use std::path::Path;
use std::time::SystemTime;

use sevenz_rust2::{ArchiveReader, Password};

use super::{ArchiveSource, ReadSeek};
use crate::ArchivePassword;
use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractionStats, ExtractionWriter, PlannedEntry};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryKind,
    EntryPathEncoding, VerificationResult,
};
use crate::security::ExtractionLimits;

pub(crate) struct SevenZipBackend {
    source: ArchiveSource,
    password: Password,
}

impl SevenZipBackend {
    pub(crate) fn new(source: ArchiveSource, password: Option<ArchivePassword>) -> Result<Self> {
        let password = password.map_or_else(
            || Ok(Password::empty()),
            |password| password.as_utf8().map(Password::new),
        )?;
        Ok(Self { source, password })
    }

    fn reader(&self) -> Result<ArchiveReader<Box<dyn ReadSeek>>> {
        ArchiveReader::new(self.source.reader()?, self.password.clone())
            .map_err(map_seven_zip_error)
    }
}

impl super::ArchiveBackend for SevenZipBackend {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::SevenZip
    }

    fn capabilities(&self) -> ArchiveCapabilities {
        self.reader().map_or_else(
            |_| ArchiveCapabilities::seven_zip(true),
            |reader| ArchiveCapabilities::seven_zip(reader.archive().is_solid),
        )
    }

    fn validate(&self) -> Result<()> {
        self.reader().map(|_| ())
    }

    fn entries(&self) -> Result<Vec<ArchiveEntry>> {
        let reader = self.reader()?;
        let encrypted = reader.archive().blocks.iter().any(|block| {
            block.coders.iter().any(|coder| {
                coder.encoder_method_id() == sevenz_rust2::EncoderMethod::ID_AES256_SHA256
            })
        });
        Ok(reader
            .archive()
            .files
            .iter()
            .enumerate()
            .map(|(index, entry)| ArchiveEntry {
                archive_index: u64::try_from(index).unwrap_or(u64::MAX),
                path: entry.name.clone(),
                path_encoding: EntryPathEncoding::Utf8,
                kind: if entry.is_directory {
                    EntryKind::Directory
                } else {
                    EntryKind::File
                },
                size: entry.size,
                compressed_size: Some(entry.compressed_size),
                modified_time: entry
                    .has_last_modified_date
                    .then(|| SystemTime::from(entry.last_modified_date))
                    .and_then(system_time_rfc3339),
                encrypted: encrypted && !entry.is_directory,
                executable: false,
                symlink_target: None,
                crc32: entry.has_crc.then(|| format!("{:08x}", entry.crc)),
                mime_guess: None,
            })
            .collect())
    }

    fn copy_entry_to(&self, path: &str, writer: &mut dyn Write) -> Result<EntryCopyResult> {
        let mut reader = self.reader()?;
        let mut found = false;
        let mut bytes_written = 0_u64;
        reader
            .for_each_entries(|entry, entry_reader| {
                if entry.name != path {
                    return Ok(true);
                }
                found = true;
                bytes_written = io::copy(entry_reader, writer)?;
                Ok(false)
            })
            .map_err(map_seven_zip_error)?;
        if !found {
            return Err(ArcthisError::EntryNotFound {
                entry: path.to_owned(),
            });
        }
        Ok(EntryCopyResult { bytes_written })
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
        let mut output = ExtractionWriter::new(staging_root, limits);
        let mut failure = None;
        let mut reader = self.reader()?;
        let result = reader.for_each_entries(|entry, entry_reader| {
            let Some(target) = targets.get(entry.name.as_str()) else {
                return Ok(true);
            };
            let write_result = match target.kind {
                EntryKind::Directory => output.create_directory(target),
                EntryKind::File => output.write_file(target, entry_reader),
                EntryKind::Symlink | EntryKind::Hardlink | EntryKind::Other => {
                    Err(ArcthisError::UnsafePath {
                        path: entry.name.clone(),
                        reason: "links and special entries cannot be extracted".to_owned(),
                    })
                }
            };
            if let Err(error) = write_result {
                failure = Some(error);
                return Err(sevenz_rust2::Error::from(io::Error::other(
                    "arcthis extraction stopped",
                )));
            }
            Ok(true)
        });
        if let Some(error) = failure {
            return Err(error);
        }
        result.map_err(map_seven_zip_error)?;
        Ok(output.stats())
    }

    fn verify(&self) -> Result<VerificationResult> {
        let mut reader = self.reader()?;
        let mut entries_checked = 0_u64;
        let mut bytes_checked = 0_u64;
        reader
            .for_each_entries(|_, entry_reader| {
                let bytes = io::copy(entry_reader, &mut io::sink())?;
                bytes_checked = bytes_checked.checked_add(bytes).ok_or_else(|| {
                    sevenz_rust2::Error::from(io::Error::other(
                        "verified byte count overflowed u64",
                    ))
                })?;
                entries_checked = entries_checked.saturating_add(1);
                Ok(true)
            })
            .map_err(map_seven_zip_error)?;
        Ok(VerificationResult {
            verified: true,
            entries_checked,
            bytes_checked,
        })
    }
}

fn map_seven_zip_error(error: sevenz_rust2::Error) -> ArcthisError {
    match error {
        sevenz_rust2::Error::PasswordRequired => ArcthisError::PasswordRequired,
        sevenz_rust2::Error::MaybeBadPassword(_) => ArcthisError::WrongPassword,
        sevenz_rust2::Error::UnsupportedCompressionMethod(message) => {
            ArcthisError::UnsupportedOperation { message }
        }
        sevenz_rust2::Error::Unsupported(message) => ArcthisError::UnsupportedOperation {
            message: message.into_owned(),
        },
        sevenz_rust2::Error::ExternalUnsupported => ArcthisError::UnsupportedOperation {
            message: "7z archive requires an unsupported external codec".to_owned(),
        },
        sevenz_rust2::Error::MaxMemLimited { max_kb, actaul_kb } => ArcthisError::ResourceLimit {
            message: format!("7z decoder requires {actaul_kb} KiB, above its {max_kb} KiB limit"),
        },
        sevenz_rust2::Error::Io(source, _) | sevenz_rust2::Error::FileOpen(source, _) => {
            ArcthisError::io("accessing 7z archive", source)
        }
        other => ArcthisError::InvalidArchive {
            message: other.to_string(),
        },
    }
}

fn system_time_rfc3339(value: SystemTime) -> Option<String> {
    let seconds = value
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())?;
    time::OffsetDateTime::from_unix_timestamp(seconds)
        .ok()?
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}
