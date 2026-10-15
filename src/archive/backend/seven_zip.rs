use std::io::{self, Write};
use std::time::SystemTime;

use sevenz_rust2::{ArchiveReader, Password};

use super::{ArchiveSource, ReadSeek};
use crate::ArchivePassword;
use crate::error::{ArcthisError, Result};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryKind,
    EntryPathEncoding, VerificationResult,
};

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
        let mut block_files = vec![0_usize; reader.archive().blocks.len()];
        for block in reader
            .archive()
            .stream_map
            .file_block_index
            .iter()
            .flatten()
        {
            if let Some(count) = block_files.get_mut(*block) {
                *count += 1;
            }
        }
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
                compressed_size: if entry.is_directory {
                    None
                } else {
                    match reader
                        .archive()
                        .stream_map
                        .file_block_index
                        .get(index)
                        .copied()
                        .flatten()
                    {
                        Some(block) if block_files.get(block) == Some(&1) => {
                            Some(entry.compressed_size)
                        }
                        None if entry.size == 0 => Some(0),
                        _ => None,
                    }
                },
                modified_time: entry
                    .has_last_modified_date
                    .then(|| SystemTime::from(entry.last_modified_date))
                    .and_then(system_time_rfc3339),
                encrypted: encrypted && !entry.is_directory,
                executable: None,
                symlink_target: None,
                crc32: entry.has_crc.then(|| format!("{:08x}", entry.crc)),
                path_raw_hex: String::new(),
                modified_time_local: None,
                mime_guess: None,
            })
            .collect())
    }

    fn enforce_compression_ratio(&self, max_ratio: u64) -> Result<()> {
        let reader = self.reader()?;
        let archive = reader.archive();
        let starts = archive.stream_map.block_first_pack_stream_index();
        for (index, block) in archive.blocks.iter().enumerate() {
            let start = starts
                .get(index)
                .copied()
                .ok_or_else(|| ArcthisError::InvalidArchive {
                    message: "7z block has no packed stream mapping".to_owned(),
                })?;
            let end = starts
                .get(index + 1)
                .copied()
                .unwrap_or(archive.pack_sizes().len());
            let packed = archive
                .pack_sizes()
                .get(start..end)
                .ok_or_else(|| ArcthisError::InvalidArchive {
                    message: "7z packed stream mapping is out of range".to_owned(),
                })?
                .iter()
                .try_fold(0_u64, |sum, size| sum.checked_add(*size))
                .ok_or_else(|| ArcthisError::ResourceLimit {
                    message: "7z packed block size overflow".to_owned(),
                })?;
            let unpacked = block.get_unpack_size();
            if unpacked > 0 && (packed == 0 || unpacked > packed.saturating_mul(max_ratio)) {
                return Err(ArcthisError::ResourceLimit {
                    message: format!(
                        "7z compression block {index} exceeds the configured {max_ratio}:1 compression ratio limit"
                    ),
                });
            }
        }
        Ok(())
    }

    fn copy_entries_to(
        &self,
        entries: &[ArchiveEntry],
        visitor: &mut super::EntryVisitor<'_>,
    ) -> Result<()> {
        let mut reader = self.reader()?;
        let targets = entries
            .iter()
            .map(|entry| (entry.archive_index, entry))
            .collect::<std::collections::HashMap<_, _>>();
        // Native callbacks borrow the immutable files table. Pointer identity preserves
        // duplicate instances even when the codec visits empty files after data blocks.
        let indices = reader
            .archive()
            .files
            .iter()
            .enumerate()
            .map(|(index, entry)| (std::ptr::from_ref(entry), index))
            .collect::<std::collections::HashMap<_, _>>();
        let mut empty = reader
            .archive()
            .files
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                reader
                    .archive()
                    .stream_map
                    .file_block_index
                    .get(*index)
                    .copied()
                    .flatten()
                    .is_none()
            })
            .filter_map(|(index, file)| {
                targets
                    .get(&u64::try_from(index).unwrap_or(u64::MAX))
                    .map(|entry| (index, file.name.clone(), *entry))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .peekable();
        let mut seen_empty = std::collections::HashSet::new();
        let mut stopped = false;
        let mut failure = None;
        let result = reader.for_each_entries(|file, source| {
            let Some(index) = indices.get(&std::ptr::from_ref(file)).copied() else {
                return Err(sevenz_rust2::Error::from(io::Error::other(
                    "unknown 7z entry identity",
                )));
            };
            while empty.peek().is_some_and(|(earlier, _, _)| *earlier < index) {
                let Some((earlier, name, entry)) = empty.next() else {
                    break;
                };
                if !seen_empty.insert(earlier) {
                    continue;
                }
                let result = visit_content(entry, &name, &mut io::empty(), visitor);
                continue_scan(result, &mut failure, &mut stopped)?;
            }
            if seen_empty.contains(&index) {
                return Ok(true);
            }
            if !file.has_stream {
                seen_empty.insert(index);
            }
            let Some(entry) = targets.get(&u64::try_from(index).unwrap_or(u64::MAX)) else {
                return Ok(true);
            };
            let result = visit_content(entry, &file.name, source, visitor);
            continue_scan(result, &mut failure, &mut stopped)
        });
        if let Some(error) = failure {
            return Err(error);
        }
        if stopped {
            return Ok(());
        }
        result.map_err(map_seven_zip_error)
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

fn visit_content(
    entry: &ArchiveEntry,
    actual_name: &str,
    source: &mut dyn std::io::Read,
    visitor: &mut super::EntryVisitor<'_>,
) -> Result<bool> {
    let mut copy = |writer: &mut dyn Write| {
        if actual_name != entry.original_path()? {
            return Err(ArcthisError::EntryNotFound {
                entry: entry.path.clone(),
            });
        }
        let bytes_written = io::copy(source, writer)
            .map_err(|error| ArcthisError::io("streaming 7z entry", error))?;
        Ok(EntryCopyResult { bytes_written })
    };
    visitor(entry, &mut copy)
}
fn continue_scan(
    result: Result<bool>,
    failure: &mut Option<ArcthisError>,
    stopped: &mut bool,
) -> std::result::Result<bool, sevenz_rust2::Error> {
    match result {
        Ok(true) => Ok(true),
        Ok(false) => {
            *stopped = true;
            Err(sevenz_rust2::Error::from(io::Error::other(
                "arcthis scan finished",
            )))
        }
        Err(error) => {
            *failure = Some(error);
            Err(sevenz_rust2::Error::from(io::Error::other(
                "arcthis scan stopped",
            )))
        }
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
