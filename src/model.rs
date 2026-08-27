use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGzip,
}

impl ArchiveFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::TarGzip => "tar_gzip",
        }
    }
}

impl fmt::Display for ArchiveFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Hardlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPathEncoding {
    Utf8,
    EscapedBytes,
}

impl fmt::Display for EntryKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Hardlink => "hardlink",
            Self::Other => "other",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchiveEntry {
    pub path: String,
    pub path_encoding: EntryPathEncoding,
    pub kind: EntryKind,
    pub size: u64,
    pub compressed_size: Option<u64>,
    pub modified_time: Option<String>,
    pub encrypted: bool,
    pub executable: bool,
    pub symlink_target: Option<String>,
    pub crc32: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(clippy::struct_excessive_bools)] // Stable JSON capability flags are intentionally independent.
pub struct ArchiveCapabilities {
    pub random_access: bool,
    pub streaming_read: bool,
    pub encrypted: bool,
    pub solid: bool,
    pub can_create: bool,
    pub can_extract: bool,
    pub can_verify: bool,
    pub can_seek: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchiveWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArchiveInspection {
    pub compression: String,
    pub encrypted: bool,
    pub solid: bool,
    pub random_access: bool,
    pub entry_count: u64,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub compression_ratio: Option<f64>,
    pub warnings: Vec<ArchiveWarning>,
    pub capabilities: ArchiveCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EntryCopyResult {
    pub bytes_written: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VerificationResult {
    pub verified: bool,
    pub entries_checked: u64,
    pub bytes_checked: u64,
}

impl ArchiveCapabilities {
    pub const fn zip() -> Self {
        Self {
            random_access: true,
            streaming_read: true,
            encrypted: false,
            solid: false,
            can_create: true,
            can_extract: true,
            can_verify: true,
            can_seek: true,
        }
    }

    pub const fn tar() -> Self {
        Self {
            random_access: false,
            streaming_read: true,
            encrypted: false,
            solid: false,
            can_create: true,
            can_extract: true,
            can_verify: true,
            can_seek: false,
        }
    }
}
