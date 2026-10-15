use std::io::{self, Write};

use zip::ZipArchive;
use zip::result::ZipError;

use super::{ArchiveBackend, ArchiveSource, EntryVisitor, ReadSeek, display_entry_path};
use crate::ArchivePassword;
use crate::error::{ArcthisError, Result};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryKind,
    VerificationResult,
};

pub(crate) struct ZipBackend {
    source: ArchiveSource,
    password: Option<ArchivePassword>,
}

impl ZipBackend {
    pub(crate) const fn new(source: ArchiveSource, password: Option<ArchivePassword>) -> Self {
        Self { source, password }
    }

    fn open(&self) -> Result<ZipArchive<Box<dyn ReadSeek>>> {
        ZipArchive::new(self.source.reader()?).map_err(map_zip_error)
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
        let start = archive.central_directory_start();
        let base = archive.offset();
        let mut source = self.source.reader()?;
        let records = super::zip_directory::records(&mut *source, start)?;
        let mut entries = Vec::with_capacity(records.len());
        if records.len() == archive.len() {
            for index in 0..archive.len() {
                let file = archive.by_index_raw(index).map_err(map_zip_error)?;
                entries.push(entry_metadata(&file, index));
            }
        } else {
            for (index, record) in records.iter().enumerate() {
                let view = super::zip_directory::EntryView::new(&mut *source, start, base, record)?;
                let mut single = ZipArchive::with_config(
                    zip::read::Config {
                        archive_offset: zip::read::ArchiveOffset::Known(base),
                    },
                    view,
                )
                .map_err(map_zip_error)?;
                let file = single.by_index_raw(0).map_err(map_zip_error)?;
                entries.push(entry_metadata(&file, index));
            }
        }
        Ok(entries)
    }

    fn copy_entries_to(
        &self,
        entries: &[ArchiveEntry],
        visitor: &mut EntryVisitor<'_>,
    ) -> Result<()> {
        let mut archive = self.open()?;
        let start = archive.central_directory_start();
        let base = archive.offset();
        let mut source = self.source.reader()?;
        let records = super::zip_directory::records(&mut *source, start)?;
        let duplicates = records.len() != archive.len();
        for entry in entries {
            let mut copy = |writer: &mut dyn Write| {
                let index = usize::try_from(entry.archive_index).map_err(|_| {
                    ArcthisError::EntryNotFound {
                        entry: entry.path.clone(),
                    }
                })?;
                if duplicates {
                    let record = records
                        .get(index)
                        .ok_or_else(|| ArcthisError::EntryNotFound {
                            entry: entry.path.clone(),
                        })?;
                    let view =
                        super::zip_directory::EntryView::new(&mut *source, start, base, record)?;
                    let mut single = ZipArchive::with_config(
                        zip::read::Config {
                            archive_offset: zip::read::ArchiveOffset::Known(base),
                        },
                        view,
                    )
                    .map_err(map_zip_error)?;
                    copy_index(&mut single, 0, entry, writer, self.password.as_ref())
                } else {
                    copy_index(&mut archive, index, entry, writer, self.password.as_ref())
                }
            };
            if !visitor(entry, &mut copy)? {
                break;
            }
        }
        Ok(())
    }

    fn verify(&self) -> Result<VerificationResult> {
        let entries = self.entries()?;
        let mut checked = 0_u64;
        let mut bytes_checked = 0_u64;
        self.copy_entries_to(&entries, &mut |entry, copy| {
            if entry.kind == EntryKind::File {
                let result = copy(&mut io::sink()).map_err(|error| {
                    if let ArcthisError::CorruptedArchive { message } = error {
                        ArcthisError::VerificationFailed { message }
                    } else {
                        error
                    }
                })?;
                bytes_checked =
                    bytes_checked
                        .checked_add(result.bytes_written)
                        .ok_or_else(|| ArcthisError::VerificationFailed {
                            message: "verified byte count overflow".to_owned(),
                        })?;
            }
            checked += 1;
            Ok(true)
        })?;
        Ok(VerificationResult {
            verified: true,
            entries_checked: checked,
            bytes_checked,
        })
    }
}

fn entry_metadata<R: std::io::Read + ?Sized>(
    file: &zip::read::ZipFile<'_, R>,
    index: usize,
) -> ArchiveEntry {
    let (path, path_encoding) = display_entry_path(file.name_raw());
    let mode = file.unix_mode();
    let is_directory = file.is_dir();
    let kind = match mode.map(|value| value & 0o170_000) {
        Some(0o120_000) => EntryKind::Symlink,
        _ if is_directory => EntryKind::Directory,
        _ => EntryKind::File,
    };
    ArchiveEntry {
        archive_index: u64::try_from(index).unwrap_or(u64::MAX),
        path,
        path_encoding,
        kind,
        size: file.size(),
        compressed_size: (kind == EntryKind::File).then_some(file.compressed_size()),
        modified_time: None,
        encrypted: file.encrypted(),
        executable: (kind == EntryKind::File)
            .then(|| mode.map(|value| value & 0o111 != 0))
            .flatten(),
        symlink_target: None,
        crc32: (kind == EntryKind::File).then(|| format!("{:08x}", file.crc32())),
        path_raw_hex: String::new(),
        modified_time_local: file
            .last_modified()
            .map(|value| value.to_string().replace(' ', "T")),
        mime_guess: None,
    }
}

fn copy_index<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
    entry: &ArchiveEntry,
    writer: &mut dyn Write,
    password: Option<&ArchivePassword>,
) -> Result<EntryCopyResult> {
    let raw = archive.by_index_raw(index).map_err(map_zip_error)?;
    if display_entry_path(raw.name_raw()).0 != entry.original_path()? {
        return Err(ArcthisError::EntryNotFound {
            entry: entry.path.clone(),
        });
    }
    drop(raw);
    let mut file = if let Some(password) = password {
        archive
            .by_index_decrypt(index, password.expose())
            .map_err(map_zip_error)?
    } else {
        archive.by_index(index).map_err(map_zip_error)?
    };
    let bytes_written = io::copy(&mut file, writer).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidData {
            ArcthisError::CorruptedArchive {
                message: error.to_string(),
            }
        } else {
            ArcthisError::io("streaming ZIP entry", error)
        }
    })?;
    Ok(EntryCopyResult { bytes_written })
}

pub(crate) fn map_zip_error(error: ZipError) -> ArcthisError {
    match error {
        ZipError::Io(source) => ArcthisError::io("reading ZIP archive", source),
        ZipError::InvalidArchive(message) => ArcthisError::InvalidArchive {
            message: message.into_owned(),
        },
        ZipError::UnsupportedArchive(ZipError::PASSWORD_REQUIRED) => ArcthisError::PasswordRequired,
        ZipError::InvalidPassword => ArcthisError::WrongPassword,
        ZipError::UnsupportedArchive(message) => ArcthisError::UnsupportedOperation {
            message: message.to_owned(),
        },
        other => ArcthisError::InvalidArchive {
            message: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    #[test]
    fn bulk_copy_reuses_the_open_zip_source() {
        let workspace = tempfile::tempdir().expect("test directory");
        let path = workspace.path().join("many.zip");
        let mut writer = ZipWriter::new(File::create(&path).expect("create ZIP"));
        for name in ["a.txt", "b.txt"] {
            writer
                .start_file(name, SimpleFileOptions::default())
                .expect("entry");
            writer.write_all(name.as_bytes()).expect("content");
        }
        writer.finish().expect("finish ZIP");
        let backend = ZipBackend::new(ArchiveSource::File(path.clone()), None);
        let entries = backend.entries().expect("metadata");
        let mut seen = 0;
        backend
            .copy_entries_to(&entries, &mut |entry, copy| {
                let mut bytes = Vec::new();
                copy(&mut bytes)?;
                assert_eq!(bytes, entry.path.as_bytes());
                seen += 1;
                if seen == 1 {
                    // Moving the pathname must not require reopening the central directory.
                    fs::rename(&path, workspace.path().join("moved.zip")).expect("move source");
                }
                Ok(true)
            })
            .expect("copy both entries through one open source");
        assert_eq!(seen, 2);
    }
}
