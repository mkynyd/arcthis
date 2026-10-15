use std::io::Write;

use compress_tools::{ArchiveContents, ArchiveIteratorBuilder, ArchivePassword as LibPassword};

use super::{ArchiveBackend, ArchiveSource};
use crate::ArchivePassword;
use crate::error::{ArcthisError, Result};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryKind,
    EntryPathEncoding, VerificationResult,
};

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
                        executable: (kind == EntryKind::File).then_some(mode & 0o111 != 0),
                        symlink_target: None,
                        crc32: None,
                        path_raw_hex: String::new(),
                        modified_time_local: None,
                        mime_guess: None,
                    });
                }
                ArchiveContents::Err(error) => return Err(map_libarchive_error(error)),
                ArchiveContents::DataChunk(_) | ArchiveContents::EndOfEntry => {}
            }
        }
        Ok(entries)
    }

    fn copy_entries_to(
        &self,
        entries: &[ArchiveEntry],
        visitor: &mut super::EntryVisitor<'_>,
    ) -> Result<()> {
        let targets = entries
            .iter()
            .map(|entry| (entry.archive_index, entry))
            .collect::<std::collections::HashMap<_, _>>();
        let mut iterator = self.iterator(None)?;
        let mut index = 0_u64;
        while let Some(item) = iterator.next() {
            match item {
                ArchiveContents::StartOfEntry(name, _) => {
                    let current = index;
                    index += 1;
                    let Some(entry) = targets.get(&current) else {
                        continue;
                    };
                    let mut copy = |writer: &mut dyn Write| {
                        if name != entry.original_path()? {
                            return Err(ArcthisError::EntryNotFound {
                                entry: entry.path.clone(),
                            });
                        }
                        let mut bytes_written = 0_u64;
                        for part in iterator.by_ref() {
                            match part {
                                ArchiveContents::DataChunk(chunk) => {
                                    writer.write_all(&chunk).map_err(|error| {
                                        ArcthisError::io("streaming RAR entry", error)
                                    })?;
                                    bytes_written = bytes_written.saturating_add(
                                        u64::try_from(chunk.len()).unwrap_or(u64::MAX),
                                    );
                                }
                                ArchiveContents::EndOfEntry => break,
                                ArchiveContents::Err(error) => {
                                    return Err(map_libarchive_error(error));
                                }
                                ArchiveContents::StartOfEntry(_, _) => {
                                    return Err(ArcthisError::InvalidArchive {
                                        message: "RAR entry ended without a boundary".to_owned(),
                                    });
                                }
                            }
                        }
                        Ok(EntryCopyResult { bytes_written })
                    };
                    if !visitor(entry, &mut copy)? {
                        break;
                    }
                }
                ArchiveContents::Err(error) => return Err(map_libarchive_error(error)),
                ArchiveContents::EndOfEntry | ArchiveContents::DataChunk(_) => {}
            }
        }
        Ok(())
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
