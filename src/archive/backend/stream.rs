use std::io::{self, Read, Write};
use std::path::Path;

use super::ArchiveSource;
use crate::archive::codec::{StreamCompression, decoder};
use crate::error::{ArcthisError, Result};
use crate::extract::{ExtractionStats, ExtractionWriter, PlannedEntry};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryKind,
    EntryPathEncoding, VerificationResult,
};
use crate::security::ExtractionLimits;

pub(crate) struct StreamBackend {
    source: ArchiveSource,
    format: ArchiveFormat,
    entry_name: String,
}

impl StreamBackend {
    pub(crate) fn new(source: ArchiveSource, format: ArchiveFormat) -> Result<Self> {
        let entry_name = derive_entry_name(source.name(), format)?;
        Ok(Self {
            source,
            format,
            entry_name,
        })
    }

    fn compression(&self) -> Result<StreamCompression> {
        match self.format {
            ArchiveFormat::Gzip => Ok(StreamCompression::Gzip),
            ArchiveFormat::Bzip2 => Ok(StreamCompression::Bzip2),
            ArchiveFormat::Xz => Ok(StreamCompression::Xz),
            ArchiveFormat::Zstd => Ok(StreamCompression::Zstd),
            _ => Err(ArcthisError::UnsupportedOperation {
                message: format!("{} is not a single-stream format", self.format),
            }),
        }
    }

    fn reader(&self) -> Result<Box<dyn Read>> {
        decoder(self.source.reader()?, self.compression()?)
    }

    fn decoded_size(&self) -> Result<u64> {
        let mut reader = self.reader()?;
        io::copy(&mut reader, &mut io::sink()).map_err(|error| ArcthisError::InvalidArchive {
            message: format!("invalid {} stream: {error}", self.format),
        })
    }
}

impl super::ArchiveBackend for StreamBackend {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn capabilities(&self) -> ArchiveCapabilities {
        ArchiveCapabilities::single_stream()
    }

    fn validate(&self) -> Result<()> {
        let mut reader = self.reader()?;
        let mut byte = [0_u8; 1];
        reader
            .read(&mut byte)
            .map_err(|error| ArcthisError::InvalidArchive {
                message: format!("invalid {} stream: {error}", self.format),
            })?;
        Ok(())
    }

    fn entries(&self) -> Result<Vec<ArchiveEntry>> {
        let compressed_size = self.source.len()?;
        Ok(vec![ArchiveEntry {
            archive_index: 0,
            path: self.entry_name.clone(),
            path_encoding: EntryPathEncoding::Utf8,
            kind: EntryKind::File,
            size: self.decoded_size()?,
            compressed_size: Some(compressed_size),
            modified_time: None,
            encrypted: false,
            executable: false,
            symlink_target: None,
            crc32: None,
            mime_guess: None,
        }])
    }

    fn copy_entry_to(&self, path: &str, writer: &mut dyn Write) -> Result<EntryCopyResult> {
        if path != self.entry_name {
            return Err(ArcthisError::EntryNotFound {
                entry: path.to_owned(),
            });
        }
        let mut reader = self.reader()?;
        let bytes_written = io::copy(&mut reader, writer)
            .map_err(|error| ArcthisError::io("streaming compressed payload", error))?;
        Ok(EntryCopyResult { bytes_written })
    }

    fn extract_plan(
        &self,
        plan: &[PlannedEntry],
        staging_root: &Path,
        limits: &ExtractionLimits,
    ) -> Result<ExtractionStats> {
        let Some(entry) = plan
            .iter()
            .find(|entry| entry.archive_path == self.entry_name)
        else {
            return Ok(ExtractionStats::default());
        };
        let mut reader = self.reader()?;
        let mut writer = ExtractionWriter::new(staging_root, limits);
        writer.write_file(entry, &mut reader)?;
        Ok(writer.stats())
    }

    fn verify(&self) -> Result<VerificationResult> {
        let mut reader = self.reader()?;
        let bytes_checked = io::copy(&mut reader, &mut io::sink()).map_err(|error| {
            ArcthisError::VerificationFailed {
                message: format!("{} stream verification failed: {error}", self.format),
            }
        })?;
        Ok(VerificationResult {
            verified: true,
            entries_checked: 1,
            bytes_checked,
        })
    }
}

fn derive_entry_name(path: &Path, format: ArchiveFormat) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ArcthisError::UnsupportedOperation {
            message: "single-stream access requires a UTF-8 source filename".to_owned(),
        })?;
    let suffix = match format {
        ArchiveFormat::Gzip => ".gz",
        ArchiveFormat::Bzip2 => ".bz2",
        ArchiveFormat::Xz => ".xz",
        ArchiveFormat::Zstd => ".zst",
        _ => {
            return Err(ArcthisError::UnsupportedOperation {
                message: format!("{format} is not a single-stream format"),
            });
        }
    };
    if name.len() > suffix.len() && name.to_ascii_lowercase().ends_with(suffix) {
        Ok(name[..name.len() - suffix.len()].to_owned())
    } else {
        Ok(format!("{name}.out"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::derive_entry_name;
    use crate::model::ArchiveFormat;

    #[test]
    fn derives_payload_name_from_matching_suffix() {
        assert_eq!(
            derive_entry_name(Path::new("report.txt.gz"), ArchiveFormat::Gzip)
                .expect("derive stream entry"),
            "report.txt"
        );
    }

    #[test]
    fn preserves_disguised_source_name_with_out_suffix() {
        assert_eq!(
            derive_entry_name(Path::new("payload.bin"), ArchiveFormat::Xz)
                .expect("derive stream entry"),
            "payload.bin.out"
        );
    }
}
