mod rar;
mod seven_zip;
mod stream;
mod tar;
mod zip;
mod zip_directory;

use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractionStats, ExtractionWriter, PlannedEntry};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryKind,
    EntryPathEncoding, VerificationResult,
};
use crate::security::ExtractionLimits;

pub(crate) use self::rar::RarBackend;
pub(crate) use self::seven_zip::SevenZipBackend;
pub(crate) use self::stream::StreamBackend;
pub(crate) use self::tar::TarBackend;
pub(crate) use self::zip::ZipBackend;

pub(crate) trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

#[derive(Debug, Clone)]
pub(crate) enum ArchiveSource {
    File(PathBuf),
    Multipart {
        paths: Arc<[PathBuf]>,
        name: PathBuf,
    },
    Memory {
        bytes: Arc<[u8]>,
        name: PathBuf,
    },
}

impl ArchiveSource {
    pub(crate) fn file(path: PathBuf) -> Self {
        Self::File(path)
    }

    pub(crate) fn memory(bytes: Vec<u8>, name: PathBuf) -> Self {
        Self::Memory {
            bytes: Arc::from(bytes),
            name,
        }
    }

    pub(crate) fn multipart(paths: Vec<PathBuf>) -> Result<Self> {
        let name =
            paths
                .first()
                .cloned()
                .ok_or_else(|| crate::ArcthisError::UnsupportedOperation {
                    message: "a multipart source requires at least one volume".to_owned(),
                })?;
        MultiFileReader::open(&paths)?;
        Ok(Self::Multipart {
            paths: Arc::from(paths),
            name,
        })
    }

    pub(crate) fn reader(&self) -> Result<Box<dyn ReadSeek>> {
        match self {
            Self::File(path) => File::open(path)
                .map(|file| Box::new(file) as Box<dyn ReadSeek>)
                .map_err(|error| crate::ArcthisError::io("opening archive source", error)),
            Self::Multipart { paths, .. } => {
                Ok(Box::new(MultiFileReader::open(paths)?) as Box<dyn ReadSeek>)
            }
            Self::Memory { bytes, .. } => Ok(Box::new(Cursor::new(Arc::clone(bytes)))),
        }
    }

    pub(crate) fn len(&self) -> Result<u64> {
        match self {
            Self::File(path) => std::fs::metadata(path)
                .map(|metadata| metadata.len())
                .map_err(|error| crate::ArcthisError::io("reading archive metadata", error)),
            Self::Multipart { paths, .. } => paths.iter().try_fold(0_u64, |total, path| {
                let size = std::fs::metadata(path)
                    .map_err(|error| {
                        crate::ArcthisError::io("reading archive volume metadata", error)
                    })?
                    .len();
                total
                    .checked_add(size)
                    .ok_or_else(|| crate::ArcthisError::ResourceLimit {
                        message: "multipart archive size overflows u64".to_owned(),
                    })
            }),
            Self::Memory { bytes, .. } => Ok(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
        }
    }

    pub(crate) fn name(&self) -> &Path {
        match self {
            Self::File(path) => path,
            Self::Multipart { name, .. } | Self::Memory { name, .. } => name,
        }
    }

    pub(crate) fn file_path(&self) -> Option<&Path> {
        match self {
            Self::File(path) => Some(path),
            Self::Multipart { .. } | Self::Memory { .. } => None,
        }
    }

    pub(crate) fn volume_count(&self) -> u64 {
        match self {
            Self::Multipart { paths, .. } => u64::try_from(paths.len()).unwrap_or(u64::MAX),
            Self::File(_) | Self::Memory { .. } => 1,
        }
    }
}

struct MultiFileReader {
    files: Vec<File>,
    offsets: Vec<u64>,
    len: u64,
    position: u64,
}

impl MultiFileReader {
    fn open(paths: &[PathBuf]) -> Result<Self> {
        if paths.is_empty() {
            return Err(crate::ArcthisError::UnsupportedOperation {
                message: "a multipart source requires at least one volume".to_owned(),
            });
        }
        let mut files = Vec::with_capacity(paths.len());
        let mut offsets = Vec::with_capacity(paths.len());
        let mut len = 0_u64;
        for path in paths {
            offsets.push(len);
            let file = File::open(path)
                .map_err(|error| crate::ArcthisError::io("opening archive volume", error))?;
            let metadata = file.metadata().map_err(|error| {
                crate::ArcthisError::io("reading archive volume metadata", error)
            })?;
            if !metadata.is_file() {
                return Err(crate::ArcthisError::UnsupportedOperation {
                    message: format!("archive volume is not a regular file: {}", path.display()),
                });
            }
            len = len.checked_add(metadata.len()).ok_or_else(|| {
                crate::ArcthisError::ResourceLimit {
                    message: "multipart archive size overflows u64".to_owned(),
                }
            })?;
            files.push(file);
        }
        Ok(Self {
            files,
            offsets,
            len,
            position: 0,
        })
    }

    fn part_index(&self, position: u64) -> Option<usize> {
        if position >= self.len {
            return None;
        }
        Some(
            self.offsets
                .partition_point(|offset| *offset <= position)
                .saturating_sub(1),
        )
    }
}

impl Read for MultiFileReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() || self.position >= self.len {
            return Ok(0);
        }
        let mut total = 0_usize;
        while total < buffer.len() && self.position < self.len {
            let index = self
                .part_index(self.position)
                .ok_or_else(|| std::io::Error::other("invalid multipart read position"))?;
            let local = self.position.saturating_sub(self.offsets[index]);
            self.files[index].seek(SeekFrom::Start(local))?;
            let read = self.files[index].read(&mut buffer[total..])?;
            if read == 0 {
                let next = index + 1;
                if next >= self.files.len() {
                    break;
                }
                self.position = self.offsets[next];
                continue;
            }
            total += read;
            self.position = self
                .position
                .saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        }
        Ok(total)
    }
}

impl Seek for MultiFileReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let target = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
            SeekFrom::End(value) => i128::from(self.len) + i128::from(value),
        };
        self.position = u64::try_from(target).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid multipart seek")
        })?;
        Ok(self.position)
    }
}

/// A deferred copy lets the consumer skip content without opening its decoder.
pub(crate) type EntryCopier<'a> = dyn FnMut(&mut dyn Write) -> Result<EntryCopyResult> + 'a;
/// Return false to stop visiting entries, including before any content is opened.
pub(crate) type EntryVisitor<'a> =
    dyn FnMut(&ArchiveEntry, &mut EntryCopier<'_>) -> Result<bool> + 'a;

pub(crate) trait ArchiveBackend: Send + Sync {
    fn format(&self) -> ArchiveFormat;
    fn capabilities(&self) -> ArchiveCapabilities;
    fn validate(&self) -> Result<()>;
    fn entries(&self) -> Result<Vec<ArchiveEntry>>;
    fn enforce_compression_ratio(&self, _max_ratio: u64) -> Result<()> {
        Ok(())
    }
    fn copy_entries_to(
        &self,
        entries: &[ArchiveEntry],
        visitor: &mut EntryVisitor<'_>,
    ) -> Result<()>;
    fn extract_plan(
        &self,
        plan: &[PlannedEntry],
        staging_root: &Path,
        limits: &ExtractionLimits,
    ) -> Result<ExtractionStats> {
        let mut output = ExtractionWriter::new(staging_root, limits);
        let targets = plan
            .iter()
            .map(|entry| (entry.archive_index, entry))
            .collect::<std::collections::HashMap<_, _>>();
        let entries = plan
            .iter()
            .map(|entry| ArchiveEntry {
                archive_index: entry.archive_index,
                path: entry.archive_path.clone(),
                path_encoding: EntryPathEncoding::Utf8,
                path_raw_hex: entry.path_raw_hex.clone(),
                kind: entry.kind,
                size: entry.size,
                compressed_size: None,
                modified_time: None,
                modified_time_local: None,
                encrypted: false,
                executable: (entry.kind == EntryKind::File).then_some(entry.executable),
                symlink_target: None,
                crc32: None,
                mime_guess: None,
            })
            .collect::<Vec<_>>();
        self.copy_entries_to(&entries, &mut |entry, copy| {
            let target =
                targets
                    .get(&entry.archive_index)
                    .ok_or_else(|| ArcthisError::EntryNotFound {
                        entry: entry.path.clone(),
                    })?;
            match target.kind {
                EntryKind::Directory => output.create_directory(target)?,
                EntryKind::File => output.write_file_with(target, |writer| {
                    copy(writer)
                        .map(|result| result.bytes_written)
                        .map_err(crate::extract::map_archive_stream_error)
                })?,
                _ => {
                    return Err(ArcthisError::UnsafePath {
                        path: entry.path.clone(),
                        reason: "links and special entries are not restored".to_owned(),
                    });
                }
            }
            Ok(true)
        })?;
        Ok(output.stats())
    }
    fn verify(&self) -> Result<VerificationResult>;
}

pub(crate) fn display_entry_path(bytes: &[u8]) -> (String, EntryPathEncoding) {
    if let Ok(path) = String::from_utf8(bytes.to_vec()) {
        (path, EntryPathEncoding::Utf8)
    } else {
        let path = bytes
            .iter()
            .map(|byte| {
                if (byte.is_ascii_graphic() && *byte != b'%') || *byte == b' ' {
                    char::from(*byte).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect();
        (path, EntryPathEncoding::EscapedBytes)
    }
}

pub(crate) fn format_unix_time(seconds: u64) -> Option<String> {
    let seconds = i64::try_from(seconds).ok()?;
    let datetime = time::OffsetDateTime::from_unix_timestamp(seconds).ok()?;
    datetime
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}
