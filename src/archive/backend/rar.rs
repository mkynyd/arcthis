use std::io::Write;
use std::path::Path;

use compress_tools::{ArchiveContents, ArchiveIteratorBuilder, ArchivePassword as LibPassword};

use super::{ArchiveBackend, ArchiveSource};
use crate::ArchivePassword;
use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractionStats, ExtractionWriter, PlannedEntry};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryKind,
    EntryPathEncoding, VerificationResult,
};
use crate::security::ExtractionLimits;

pub(crate) struct RarBackend {
    source: ArchiveSource,
    password: Option<ArchivePassword>,
}

impl RarBackend {
    pub(crate) const fn new(source: ArchiveSource, password: Option<ArchivePassword>) -> Self {
        Self { source, password }
    }

    fn iterator(
        &self,
        filter: Option<String>,
    ) -> Result<compress_tools::ArchiveIterator<Box<dyn super::ReadSeek>>> {
        let mut builder = ArchiveIteratorBuilder::new(self.source.reader()?).mtree_format(false);
        if let Some(path) = filter {
            builder = builder.filter(move |name, _| name == path);
        }
        if let Some(password) = &self.password {
            let password = password.as_utf8()?;
            builder =
                builder.with_password(LibPassword::new(password).map_err(map_libarchive_error)?);
        }
        builder.build().map_err(map_libarchive_error)
    }

    fn stream_entry(&self, path: &str, writer: &mut dyn Write) -> Result<u64> {
        let mut found = false;
        let mut bytes_written = 0_u64;
        for item in self.iterator(Some(path.to_owned()))? {
            match item {
                ArchiveContents::StartOfEntry(name, _) => found = name == path,
                ArchiveContents::DataChunk(chunk) if found => {
                    writer
                        .write_all(&chunk)
                        .map_err(|error| ArcthisError::io("streaming RAR entry", error))?;
                    bytes_written = bytes_written.saturating_add(chunk.len() as u64);
                }
                ArchiveContents::EndOfEntry | ArchiveContents::DataChunk(_) => {}
                ArchiveContents::Err(error) => return Err(map_libarchive_error(error)),
            }
        }
        if !found {
            return Err(ArcthisError::EntryNotFound {
                entry: path.to_owned(),
            });
        }
        Ok(bytes_written)
    }
}

impl ArchiveBackend for RarBackend {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Rar
    }

    fn capabilities(&self) -> ArchiveCapabilities {
        ArchiveCapabilities::rar()
    }

    fn validate(&self) -> Result<()> {
        self.iterator(None).map(|_| ())
    }

    fn entries(&self) -> Result<Vec<ArchiveEntry>> {
        let mut entries = Vec::new();
        for item in self.iterator(None)? {
            match item {
                ArchiveContents::StartOfEntry(path, stat) => {
                    let mode = u64::from(stat.st_mode);
                    let kind = if mode & 0o170_000 == 0o040_000 || path.ends_with('/') {
                        EntryKind::Directory
                    } else if mode & 0o170_000 == 0o120_000 {
                        EntryKind::Symlink
                    } else {
                        EntryKind::File
                    };
                    entries.push(ArchiveEntry {
                        archive_index: u64::try_from(entries.len()).unwrap_or(u64::MAX),
                        path,
                        path_encoding: EntryPathEncoding::Utf8,
                        kind,
                        size: u64::try_from(stat.st_size).unwrap_or(0),
                        compressed_size: None,
                        modified_time: u64::try_from(stat.st_mtime)
                            .ok()
                            .and_then(super::format_unix_time),
                        encrypted: false,
                        executable: mode & 0o111 != 0,
                        symlink_target: None,
                        crc32: None,
                        mime_guess: None,
                    });
                }
                ArchiveContents::Err(error) => return Err(map_libarchive_error(error)),
                ArchiveContents::DataChunk(_) | ArchiveContents::EndOfEntry => {}
            }
        }
        Ok(entries)
    }

    fn copy_entry_to(&self, path: &str, writer: &mut dyn Write) -> Result<EntryCopyResult> {
        self.stream_entry(path, writer)
            .map(|bytes_written| EntryCopyResult { bytes_written })
    }

    fn extract_plan(
        &self,
        plan: &[PlannedEntry],
        staging_root: &Path,
        limits: &ExtractionLimits,
    ) -> Result<ExtractionStats> {
        let mut output = ExtractionWriter::new(staging_root, limits);
        for entry in plan {
            match entry.kind {
                EntryKind::Directory => output.create_directory(entry)?,
                EntryKind::File => output.write_file_with(entry, |writer| {
                    self.stream_entry(&entry.archive_path, writer)
                })?,
                EntryKind::Symlink | EntryKind::Hardlink | EntryKind::Other => {
                    return Err(ArcthisError::UnsafePath {
                        path: entry.archive_path.clone(),
                        reason: "links and special entries cannot be extracted".to_owned(),
                    });
                }
            }
        }
        Ok(output.stats())
    }

    fn verify(&self) -> Result<VerificationResult> {
        let mut entries_checked = 0_u64;
        let mut bytes_checked = 0_u64;
        for item in self.iterator(None)? {
            match item {
                ArchiveContents::StartOfEntry(_, _) => {
                    entries_checked = entries_checked.saturating_add(1);
                }
                ArchiveContents::DataChunk(chunk) => {
                    bytes_checked = bytes_checked.saturating_add(chunk.len() as u64);
                }
                ArchiveContents::Err(error) => return Err(map_libarchive_error(error)),
                ArchiveContents::EndOfEntry => {}
            }
        }
        Ok(VerificationResult {
            verified: true,
            entries_checked,
            bytes_checked,
        })
    }
}

fn map_libarchive_error(error: compress_tools::Error) -> ArcthisError {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    if lower.contains("passphrase") || lower.contains("password") {
        if lower.contains("required") || lower.contains("please set") {
            return ArcthisError::PasswordRequired;
        }
        return ArcthisError::WrongPassword;
    }
    if let compress_tools::Error::Io(source) = error {
        return ArcthisError::io("reading RAR archive", source);
    }
    if lower.contains("unsupported") {
        return ArcthisError::UnsupportedOperation { message };
    }
    ArcthisError::InvalidArchive { message }
}
