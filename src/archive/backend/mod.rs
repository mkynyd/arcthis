mod tar;
mod zip;

use std::io::Write;
use std::path::Path;

use crate::error::Result;
use crate::extract::{ExtractionStats, PlannedEntry};
use crate::model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, EntryCopyResult, EntryPathEncoding,
    VerificationResult,
};
use crate::security::ExtractionLimits;

pub(crate) use self::tar::TarBackend;
pub(crate) use self::zip::ZipBackend;

pub(crate) trait ArchiveBackend: Send + Sync {
    fn format(&self) -> ArchiveFormat;
    fn capabilities(&self) -> ArchiveCapabilities;
    fn validate(&self) -> Result<()>;
    fn entries(&self) -> Result<Vec<ArchiveEntry>>;
    fn copy_entry_to(&self, path: &str, writer: &mut dyn Write) -> Result<EntryCopyResult>;
    fn extract_plan(
        &self,
        plan: &[PlannedEntry],
        staging_root: &Path,
        limits: &ExtractionLimits,
    ) -> Result<ExtractionStats>;
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
